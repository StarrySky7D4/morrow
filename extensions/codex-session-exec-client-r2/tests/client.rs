use morrow_agent_session_exec_v1_r2::authority::{
    Admission, Capabilities as HostCaps, SessionExecHost,
};
use morrow_codex_session_exec_client_r2::{
    protocol::{Action, Event, Outcome, Reply, Request, hash},
    *,
};
use morrow_core::{
    dispatch::{Connection, HostRuntime},
    store::{EventBudget, Store},
};
use std::{cell::Cell, rc::Rc};

struct Fixture {
    _temp: tempfile::TempDir,
    runtime: HostRuntime,
    connection: Connection,
    host: SessionExecHost,
    admission: Admission,
    calls: usize,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let mut runtime = HostRuntime::new(
            Store::open(&temp.path().join("client.sqlite"), EventBudget::default()).unwrap(),
        )
        .unwrap();
        let connection = runtime.connect().unwrap();
        let host = SessionExecHost::new(&mut runtime).unwrap();
        let caps = HostCaps {
            session_read: true,
            session_write: true,
            propose: true,
            execute: true,
            retire: false,
        };
        let admission = host
            .admit(
                &runtime,
                &connection,
                caps,
                caps,
                vec!["session".into(), "child".into()],
                "test-domain".into(),
                100,
                1,
            )
            .unwrap();
        Self {
            _temp: temp,
            runtime,
            connection,
            host,
            admission,
            calls: 0,
        }
    }
}
impl Transport for Fixture {
    fn exchange_once(&mut self, bytes: &[u8]) -> std::result::Result<Vec<u8>, ()> {
        self.calls += 1;
        self.host
            .dispatch(
                &mut self.runtime,
                &self.connection,
                &self.admission,
                bytes,
                || 1,
            )
            .map_err(|_| ())
    }
}
fn scope() -> Scope {
    Scope {
        capabilities: Capabilities {
            read: true,
            write: true,
            propose: true,
            execute: true,
        },
        sessions: vec!["session".into(), "child".into()],
        operations: vec!["op".into()],
        execution_domain: "test-domain".into(),
    }
}
fn budget() -> Budget {
    Budget {
        calls: 64,
        request_bytes: 4 * 1024 * 1024,
        reply_bytes: 4 * 1024 * 1024,
    }
}
fn client() -> Client<Fixture> {
    Client::new(Fixture::new(), 1, [7; 16], scope(), budget()).unwrap()
}
fn event(n: usize) -> Event {
    Event {
        event_id: format!("event-{n}"),
        body: vec![n as u8, 0, 255],
    }
}

#[test]
fn sqlite_session_writer_checkpoint_sealed_continuation_archive() {
    let mut c = client();
    c.create("session").unwrap();
    let mut writer = c.open_writer("session", 0).unwrap();
    c.append(&mut writer, vec![event(1)]).unwrap();
    let unsealed = c.snapshot("session", 0, 16).unwrap();
    assert_eq!(c.continue_sealed(&unsealed, "child"), Err(Error::Unsealed));
    c.checkpoint(&mut writer, b"opaque\0\xff".to_vec()).unwrap();
    let parent = c.snapshot("session", 0, 16).unwrap();
    c.continue_sealed(&parent, "child").unwrap();
    let child = c.snapshot("child", 0, 16).unwrap();
    assert_eq!(child.checkpoint, parent.checkpoint);
    assert!(child.events.is_empty());
    assert_eq!(
        child.info.parent_checkpoint_sha256,
        Some(hash(b"opaque\0\xff"))
    );
    c.archive(&mut writer).unwrap();
    assert!(!writer.is_valid());
    assert_eq!(
        c.append(&mut writer, vec![event(2)]),
        Err(Error::StaleWriter)
    );
    assert_eq!(c.list().unwrap().len(), 2);
}
#[test]
fn paginated_history_preserves_all_binary_events_and_enforces_bounds() {
    let mut c = client();
    c.create("session").unwrap();
    let mut w = c.open_writer("session", 0).unwrap();
    for n in 0..3 {
        c.append(&mut w, (n * 16..(n + 1) * 16).map(event).collect())
            .unwrap();
    }
    let history = c
        .history(
            "session",
            0,
            HistoryLimits {
                page_size: 5,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(history.events.len(), 48);
    assert!(!history.gap);
    assert_eq!(history.events.last().unwrap().sequence, 48);
    assert!(matches!(
        c.history(
            "session",
            0,
            HistoryLimits {
                events: 7,
                ..Default::default()
            }
        ),
        Err(Error::Budget)
    ));
    assert!(matches!(
        c.history(
            "session",
            0,
            HistoryLimits {
                bytes: 3,
                ..Default::default()
            }
        ),
        Err(Error::Budget)
    ));
    assert!(matches!(
        c.history(
            "session",
            0,
            HistoryLimits {
                pages: 1,
                ..Default::default()
            }
        ),
        Err(Error::Budget)
    ));
}
#[test]
fn compacted_history_explicitly_returns_the_sealed_baseline() {
    let mut c = client();
    c.create("session").unwrap();
    let mut w = c.open_writer("session", 0).unwrap();
    c.append(&mut w, vec![event(1), event(2)]).unwrap();
    c.checkpoint(&mut w, b"baseline".to_vec()).unwrap();
    let mut f = c.into_transport();
    f.host
        .compact(&mut f.runtime, &f.connection, &f.admission, "session", 2, 1)
        .unwrap();
    let mut c = Client::new(f, 1, [8; 16], scope(), budget()).unwrap();
    let history = c.history("session", 0, HistoryLimits::default()).unwrap();
    assert!(history.gap);
    assert_eq!(history.checkpoint, b"baseline");
    assert!(history.events.is_empty());
    c.append(&mut w, vec![event(3), event(4)]).unwrap();
    let history = c
        .history(
            "session",
            0,
            HistoryLimits {
                page_size: 1,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(history.gap);
    assert_eq!(history.checkpoint, b"baseline");
    assert_eq!(
        history
            .events
            .iter()
            .map(|e| e.sequence)
            .collect::<Vec<_>>(),
        vec![3, 4]
    );
    assert!(!format!("{history:?}").contains("baseline"));
}
#[test]
fn declaration_and_budget_fail_before_transport() {
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    let transport = move |_: &[u8]| {
        counted.set(counted.get() + 1);
        Err(())
    };
    let mut s = scope();
    s.capabilities.write = false;
    let mut c = Client::new(
        transport,
        1,
        [1; 16],
        s,
        Budget {
            reply_bytes: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(c.create("session"), Err(Error::Denied));
    assert!(matches!(c.snapshot("session", 0, 16), Err(Error::Budget)));
    assert!(matches!(c.snapshot("foreign", 0, 16), Err(Error::Denied)));
    assert_eq!(calls.get(), 0);
}
#[test]
fn uncertain_call_is_once_and_failed_writer_cannot_replay() {
    let mut fixture = Fixture::new();
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    let transport = move |bytes: &[u8]| {
        counted.set(counted.get() + 1);
        let reply = fixture.exchange_once(bytes)?;
        if matches!(
            Request::decode(bytes).unwrap().action(),
            Action::Append { .. }
        ) {
            Err(())
        } else {
            Ok(reply)
        }
    };
    let mut c = Client::new(transport, 1, [1; 16], scope(), budget()).unwrap();
    c.create("session").unwrap();
    let mut w = c.open_writer("session", 0).unwrap();
    assert!(matches!(
        c.append(&mut w, vec![event(1)]),
        Err(Error::Unknown(_))
    ));
    assert_eq!(calls.get(), 3);
    assert_eq!(c.append(&mut w, vec![event(1)]), Err(Error::StaleWriter));
    assert_eq!(calls.get(), 3);
    let snapshot = c.snapshot("session", 0, 16).unwrap();
    assert_eq!(snapshot.info.tail, 1);
    assert_eq!(snapshot.events.len(), 1);
    assert_eq!(calls.get(), 4);
}
#[test]
fn altered_generation_or_request_digest_reply_is_unknown_without_retry() {
    for generation in [1, 2] {
        let calls = Rc::new(Cell::new(0));
        let count = calls.clone();
        let transport = move |bytes: &[u8]| {
            count.set(count.get() + 1);
            let request = Request::decode(bytes).unwrap();
            let mut reply =
                Reply::new(&request, Outcome::Rejected(protocol::Error::Denied)).unwrap();
            if generation == 2 {
                reply.generation = generation;
            } else {
                reply.request_sha256 = [9; 32];
            }
            Ok(reply.encode().unwrap())
        };
        let mut c = Client::new(transport, 1, [1; 16], scope(), budget()).unwrap();
        assert!(matches!(c.create("session"), Err(Error::Unknown(_))));
        assert_eq!(calls.get(), 1);
    }
}
#[test]
fn commit_unknown_rejection_never_replays_or_hides_status() {
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let transport = move |bytes: &[u8]| {
        count.set(count.get() + 1);
        Reply::new(
            &Request::decode(bytes).unwrap(),
            Outcome::Rejected(protocol::Error::CommitUnknown),
        )
        .unwrap()
        .encode()
        .map_err(|_| ())
    };
    let mut c = Client::new(transport, 1, [2; 16], scope(), budget()).unwrap();
    assert_eq!(
        c.create("session"),
        Err(Error::Rejected(protocol::Error::CommitUnknown))
    );
    assert_eq!(calls.get(), 1);
}
#[test]
fn fixed_generation_stale_at_host_is_not_silently_refreshed() {
    let mut c = Client::new(Fixture::new(), 2, [3; 16], scope(), budget()).unwrap();
    assert!(c.create("session").is_err());
    assert_eq!(c.generation(), 2);
    assert_eq!(c.into_transport().calls, 1);
}
#[test]
fn execution_proposal_inspection_and_domain_ceiling_do_not_approve_execution() {
    let mut c = client();
    c.create("session").unwrap();
    let mut writer = c.open_writer("session", 0).unwrap();
    c.append(&mut writer, vec![event(1)]).unwrap();
    let mut intent = protocol::Intent {
        operation_id: "op".into(),
        artifact_sha256: hash(b"fixed artifact"),
        program: "C:\\synthetic\\program.exe".into(),
        argv: vec!["fixed".into()],
        cwd: "C:\\synthetic".into(),
        env: vec![],
        input: vec![],
        execution_domain: "foreign-domain".into(),
        max_runtime_ms: 100,
    };
    assert!(matches!(
        c.propose("session", intent.clone()),
        Err(Error::Denied)
    ));
    intent.execution_domain = "test-domain".into();
    let proposal = c.propose("session", intent).unwrap();
    assert_eq!(proposal.phase, protocol::ToolPhase::Proposed);
    assert_eq!(
        c.inspect("op").unwrap().phase,
        protocol::ToolPhase::Proposed
    );
    assert!(c.claim("op", [1; 32]).is_err());
    assert_eq!(
        c.inspect("op").unwrap().phase,
        protocol::ToolPhase::Proposed
    );
}
#[test]
fn history_rejects_a_revision_change_between_pages() {
    let mut fixture = Fixture::new();
    let mut original = |b: &[u8]| fixture.exchange_once(b);
    let mut c = Client::new(&mut original, 1, [4; 16], scope(), budget()).unwrap();
    c.create("session").unwrap();
    let mut w = c.open_writer("session", 0).unwrap();
    c.append(&mut w, vec![event(1), event(2)]).unwrap();
    drop(c);
    drop(original);
    let count = Cell::new(0);
    let mut c = Client::new(
        |bytes: &[u8]| {
            let request = Request::decode(bytes).unwrap();
            let reply = fixture.exchange_once(bytes)?;
            if matches!(request.action(), Action::Snapshot { .. }) {
                count.set(count.get() + 1);
                if count.get() == 1 {
                    let mutation = Request::new(
                        "interleaved",
                        Action::Append {
                            session_id: "session".into(),
                            epoch: 1,
                            expected_tail: 2,
                            events: vec![event(3)],
                        },
                    )
                    .unwrap();
                    fixture.exchange_once(mutation.raw())?;
                }
            }
            Ok(reply)
        },
        1,
        [5; 16],
        scope(),
        budget(),
    )
    .unwrap();
    assert!(matches!(
        c.history(
            "session",
            0,
            HistoryLimits {
                page_size: 1,
                ..Default::default()
            }
        ),
        Err(Error::HistoryChanged)
    ));
}
