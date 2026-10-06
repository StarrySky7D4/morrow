use morrow_agent_session_exec_v1::{
    Action, Error, Event, Outcome, Reply, Request,
    authority::{Admission, Capabilities, SessionExecHost},
    hash,
};
use morrow_core::{
    dispatch::{Connection, HostRuntime},
    store::{EventBudget, Store},
};

const SESSION: &str = "session-a";
fn caps() -> Capabilities {
    Capabilities {
        session_read: true,
        session_write: true,
        propose: true,
        execute: true,
    }
}
struct Fixture {
    temp: tempfile::TempDir,
    runtime: HostRuntime,
    connection: Connection,
    host: SessionExecHost,
    admission: Admission,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let mut runtime = HostRuntime::new(
            Store::open(&temp.path().join("original.sqlite"), EventBudget::default()).unwrap(),
        )
        .unwrap();
        let connection = runtime.connect().unwrap();
        let host = SessionExecHost::new(&mut runtime).unwrap();
        let admission = host
            .admit(
                &runtime,
                &connection,
                caps(),
                caps(),
                vec![SESSION.into(), "child".into()],
                "synthetic-local".into(),
                100,
                0,
            )
            .unwrap();
        Self {
            temp,
            runtime,
            connection,
            host,
            admission,
        }
    }
    fn send(&mut self, id: &str, action: Action) -> Outcome {
        let request = Request::new(id, action).unwrap();
        self.request(&request)
    }
    fn request(&mut self, request: &Request) -> Outcome {
        let bytes = self
            .host
            .dispatch(
                &mut self.runtime,
                &self.connection,
                &self.admission,
                request.raw(),
                || 1,
            )
            .unwrap();
        Reply::decode_for(request, &bytes).unwrap().outcome
    }
    fn create(&mut self) {
        assert!(matches!(
            self.send(
                "create",
                Action::Create {
                    session_id: SESSION.into(),
                    parent: None,
                    parent_tail: 0
                }
            ),
            Outcome::Session(_)
        ));
    }
    fn writer(&mut self) {
        self.create();
        assert!(
            matches!(self.send("writer",Action::OpenWriter{session_id:SESSION.into(),expected_epoch:0}),Outcome::Session(info) if info.epoch==1)
        );
    }
    fn append(&mut self, id: &str, tail: u64, body: Vec<u8>) -> Outcome {
        self.send(
            id,
            Action::Append {
                session_id: SESSION.into(),
                epoch: 1,
                expected_tail: tail,
                events: vec![Event {
                    event_id: id.into(),
                    body,
                }],
            },
        )
    }
    fn snapshot(&mut self, after: u64, limit: u32) -> Outcome {
        self.send(
            "read",
            Action::Snapshot {
                session_id: SESSION.into(),
                after,
                limit,
            },
        )
    }
}

#[test]
fn durable_history_checkpoint_fork_archive_and_reopen() {
    let mut f = Fixture::new();
    f.writer();
    assert!(
        matches!(f.append("event-1",0,b"opaque\0\xff".to_vec()),Outcome::Session(info) if info.tail==1)
    );
    assert!(
        matches!(f.send("seal",Action::Checkpoint{session_id:SESSION.into(),epoch:1,expected_tail:1,state:b"model checkpoint".to_vec()}),Outcome::Session(info) if info.checkpoint_sealed && info.checkpoint_sha256==hash(b"model checkpoint"))
    );
    assert!(
        matches!(f.send("fork",Action::Create{session_id:"child".into(),parent:Some(SESSION.into()),parent_tail:1}),Outcome::Session(info) if info.parent_tail==1 && info.tail==0)
    );
    assert!(
        matches!(f.send("archive",Action::Archive{session_id:SESSION.into(),epoch:1,expected_tail:1}),Outcome::Session(info) if info.archived)
    );
    assert_eq!(
        f.append("event-2", 1, b"denied".to_vec()),
        Outcome::Rejected(Error::Denied)
    );
    let path = f.temp.path().join("original.sqlite");
    drop(f.runtime);
    let mut runtime =
        HostRuntime::new(Store::open(&path, EventBudget::default()).unwrap()).unwrap();
    let connection = runtime.connect().unwrap();
    let host = SessionExecHost::new(&mut runtime).unwrap();
    let admission = host
        .admit(
            &runtime,
            &connection,
            caps(),
            caps(),
            vec![SESSION.into(), "child".into()],
            "synthetic-local".into(),
            100,
            0,
        )
        .unwrap();
    let req = Request::new(
        "after-reopen",
        Action::Snapshot {
            session_id: SESSION.into(),
            after: 0,
            limit: 16,
        },
    )
    .unwrap();
    let bytes = host
        .dispatch(&mut runtime, &connection, &admission, req.raw(), || 1)
        .unwrap();
    assert!(
        matches!(Reply::decode_for(&req,&bytes).unwrap().outcome,Outcome::Snapshot(s) if s.info.archived && s.events[0].event.body==b"opaque\0\xff" && s.checkpoint==b"model checkpoint")
    );
    assert!(runtime.store_local().card(SESSION).unwrap().is_none());
    runtime.store_local().integrity_check().unwrap();
}

#[test]
fn append_exact_original_frame_is_idempotent_but_changed_frame_conflicts() {
    let mut f = Fixture::new();
    f.writer();
    let req = Request::new(
        "batch",
        Action::Append {
            session_id: SESSION.into(),
            epoch: 1,
            expected_tail: 0,
            events: vec![Event {
                event_id: "unique".into(),
                body: b"first".to_vec(),
            }],
        },
    )
    .unwrap();
    let first = f.request(&req);
    assert_eq!(f.request(&req), first);
    let changed = Request::new(
        "batch",
        Action::Append {
            session_id: SESSION.into(),
            epoch: 1,
            expected_tail: 0,
            events: vec![Event {
                event_id: "unique".into(),
                body: b"changed".to_vec(),
            }],
        },
    )
    .unwrap();
    assert_eq!(f.request(&changed), Outcome::Rejected(Error::Conflict));
    assert!(matches!(f.snapshot(0,16),Outcome::Snapshot(s) if s.info.tail==1 && s.events.len()==1));
}

#[test]
fn handoff_requires_epoch_cas_and_invalidates_original_writer() {
    let mut f = Fixture::new();
    f.writer();
    let other = f
        .host
        .admit(
            &f.runtime,
            &f.connection,
            caps(),
            caps(),
            vec![SESSION.into()],
            "synthetic-local".into(),
            100,
            0,
        )
        .unwrap();
    let req = Request::new(
        "handoff",
        Action::OpenWriter {
            session_id: SESSION.into(),
            expected_epoch: 1,
        },
    )
    .unwrap();
    let bytes = f
        .host
        .dispatch(&mut f.runtime, &f.connection, &other, req.raw(), || 1)
        .unwrap();
    assert!(
        matches!(Reply::decode_for(&req,&bytes).unwrap().outcome,Outcome::Session(info) if info.epoch==2)
    );
    assert_eq!(
        f.append("stale", 0, vec![]),
        Outcome::Rejected(Error::Conflict)
    );
    let stolen = Request::new(
        "steal",
        Action::Append {
            session_id: SESSION.into(),
            epoch: 2,
            expected_tail: 0,
            events: vec![Event {
                event_id: "stolen".into(),
                body: vec![],
            }],
        },
    )
    .unwrap();
    assert_eq!(f.request(&stolen), Outcome::Rejected(Error::Denied));
    assert_eq!(
        f.send(
            "bad-handoff",
            Action::OpenWriter {
                session_id: SESSION.into(),
                expected_epoch: 1
            }
        ),
        Outcome::Rejected(Error::Conflict)
    );
    assert!(
        matches!(f.send("writer",Action::OpenWriter{session_id:SESSION.into(),expected_epoch:0}),Outcome::Session(info) if info.epoch==1)
    );
    assert!(matches!(f.snapshot(0,16),Outcome::Snapshot(s) if s.info.epoch==2));
}

#[test]
fn sealed_checkpoint_retention_reports_gap_and_never_reuses_event_id() {
    let mut f = Fixture::new();
    f.writer();
    f.append("a", 0, b"a".to_vec());
    f.append("b", 1, b"b".to_vec());
    assert_eq!(
        f.host
            .compact(&mut f.runtime, &f.connection, &f.admission, SESSION, 2, 1),
        Err(Error::Conflict)
    );
    f.send(
        "seal",
        Action::Checkpoint {
            session_id: SESSION.into(),
            epoch: 1,
            expected_tail: 2,
            state: b"checkpoint".to_vec(),
        },
    );
    assert_eq!(
        f.host
            .compact(&mut f.runtime, &f.connection, &f.admission, SESSION, 2, 1)
            .unwrap()
            .floor,
        3
    );
    assert!(
        matches!(f.snapshot(0,16),Outcome::Snapshot(s) if s.gap && s.events.is_empty() && s.info.tail==2 && s.checkpoint==b"checkpoint")
    );
    assert!(matches!(f.snapshot(2,16),Outcome::Snapshot(s) if !s.gap));
    assert_eq!(
        f.send(
            "reuse",
            Action::Append {
                session_id: SESSION.into(),
                epoch: 1,
                expected_tail: 2,
                events: vec![Event {
                    event_id: "a".into(),
                    body: b"a".to_vec()
                }]
            }
        ),
        Outcome::Rejected(Error::Conflict)
    );
    f.append("c", 2, b"c".to_vec());
    assert!(matches!(f.snapshot(0,16),Outcome::Snapshot(s) if s.gap && s.events[0].sequence==3));
}

#[test]
fn checkpoint_fence_is_exact_and_immutable_at_the_same_tail() {
    let mut f = Fixture::new();
    f.writer();
    f.append("a", 0, vec![]);
    assert_eq!(
        f.send(
            "bad-seal",
            Action::Checkpoint {
                session_id: SESSION.into(),
                epoch: 1,
                expected_tail: 0,
                state: vec![]
            }
        ),
        Outcome::Rejected(Error::Conflict)
    );
    assert_eq!(
        f.send(
            "unsealed-fork",
            Action::Create {
                session_id: "child".into(),
                parent: Some(SESSION.into()),
                parent_tail: 1
            }
        ),
        Outcome::Rejected(Error::Conflict)
    );
    f.send(
        "seal",
        Action::Checkpoint {
            session_id: SESSION.into(),
            epoch: 1,
            expected_tail: 1,
            state: b"fixed".to_vec(),
        },
    );
    assert_eq!(
        f.send(
            "replace",
            Action::Checkpoint {
                session_id: SESSION.into(),
                epoch: 1,
                expected_tail: 1,
                state: b"changed".to_vec()
            }
        ),
        Outcome::Rejected(Error::Conflict)
    );
    assert_eq!(
        f.send(
            "bad-fork",
            Action::Create {
                session_id: "child".into(),
                parent: Some(SESSION.into()),
                parent_tail: 0
            }
        ),
        Outcome::Rejected(Error::Conflict)
    );
    assert_eq!(f.snapshot(2, 16), Outcome::Rejected(Error::Conflict));
}

#[test]
fn admission_ceilings_scope_connection_and_revocation_are_enforced() {
    let mut f = Fixture::new();
    f.writer();
    let read = Capabilities {
        session_read: true,
        ..Capabilities::default()
    };
    assert!(matches!(
        f.host.admit(
            &f.runtime,
            &f.connection,
            read,
            caps(),
            vec![SESSION.into()],
            "domain".into(),
            100,
            0
        ),
        Err(Error::Denied)
    ));
    let reader = f
        .host
        .admit(
            &f.runtime,
            &f.connection,
            read,
            read,
            vec![SESSION.into()],
            "domain".into(),
            100,
            0,
        )
        .unwrap();
    let req = Request::new(
        "forbidden-write",
        Action::OpenWriter {
            session_id: SESSION.into(),
            expected_epoch: 1,
        },
    )
    .unwrap();
    let bytes = f
        .host
        .dispatch(&mut f.runtime, &f.connection, &reader, req.raw(), || 1)
        .unwrap();
    assert_eq!(
        Reply::decode_for(&req, &bytes).unwrap().outcome,
        Outcome::Rejected(Error::Denied)
    );
    assert_eq!(
        f.send(
            "outside",
            Action::Create {
                session_id: "not-nominated".into(),
                parent: None,
                parent_tail: 0
            }
        ),
        Outcome::Rejected(Error::Denied)
    );
    let foreign = f.runtime.connect().unwrap();
    let req = Request::new("foreign", Action::List).unwrap();
    let bytes = f
        .host
        .dispatch(&mut f.runtime, &foreign, &f.admission, req.raw(), || 1)
        .unwrap();
    assert_eq!(
        Reply::decode_for(&req, &bytes).unwrap().outcome,
        Outcome::Rejected(Error::Denied)
    );
    f.host.revoke(&f.admission).unwrap();
    assert_eq!(f.snapshot(0, 16), Outcome::Rejected(Error::Denied));
}

#[test]
fn restart_never_restores_writer_or_original_admission() {
    let mut f = Fixture::new();
    f.writer();
    let path = f.temp.path().join("original.sqlite");
    drop(f.runtime);
    let mut runtime =
        HostRuntime::new(Store::open(&path, EventBudget::default()).unwrap()).unwrap();
    let connection = runtime.connect().unwrap();
    let host = SessionExecHost::new(&mut runtime).unwrap();
    let admission = host
        .admit(
            &runtime,
            &connection,
            caps(),
            caps(),
            vec![SESSION.into()],
            "domain".into(),
            100,
            0,
        )
        .unwrap();
    let req = Request::new(
        "old-writer",
        Action::Append {
            session_id: SESSION.into(),
            epoch: 1,
            expected_tail: 0,
            events: vec![Event {
                event_id: "a".into(),
                body: vec![],
            }],
        },
    )
    .unwrap();
    let out = host
        .dispatch(&mut runtime, &connection, &admission, req.raw(), || 1)
        .unwrap();
    assert_eq!(
        Reply::decode_for(&req, &out).unwrap().outcome,
        Outcome::Rejected(Error::Denied)
    );
    let out = host
        .dispatch(&mut runtime, &connection, &f.admission, req.raw(), || 1)
        .unwrap();
    assert_eq!(
        Reply::decode_for(&req, &out).unwrap().outcome,
        Outcome::Rejected(Error::Denied)
    );
    let req = Request::new(
        "new-writer",
        Action::OpenWriter {
            session_id: SESSION.into(),
            expected_epoch: 1,
        },
    )
    .unwrap();
    let out = host
        .dispatch(&mut runtime, &connection, &admission, req.raw(), || 1)
        .unwrap();
    assert!(
        matches!(Reply::decode_for(&req,&out).unwrap().outcome,Outcome::Session(info) if info.epoch==2)
    );
}

#[test]
fn final_read_expiry_withholds_plaintext_and_lost_write_ack_is_unknown() {
    let mut f = Fixture::new();
    f.writer();
    f.append("a", 0, b"must not escape".to_vec());
    let req = Request::new(
        "expiring-read",
        Action::Snapshot {
            session_id: SESSION.into(),
            after: 0,
            limit: 16,
        },
    )
    .unwrap();
    let mut ticks = [1, 100].into_iter();
    let bytes = f
        .host
        .dispatch(
            &mut f.runtime,
            &f.connection,
            &f.admission,
            req.raw(),
            || ticks.next().unwrap(),
        )
        .unwrap();
    assert_eq!(
        Reply::decode_for(&req, &bytes).unwrap().outcome,
        Outcome::Rejected(Error::Denied)
    );
    assert!(
        !bytes
            .windows(b"must not escape".len())
            .any(|b| b == b"must not escape")
    );
    let req = Request::new(
        "lost-ack",
        Action::Append {
            session_id: SESSION.into(),
            epoch: 1,
            expected_tail: 1,
            events: vec![Event {
                event_id: "b".into(),
                body: b"committed".to_vec(),
            }],
        },
    )
    .unwrap();
    let mut ticks = [1, 1, 100].into_iter();
    let bytes = f
        .host
        .dispatch(
            &mut f.runtime,
            &f.connection,
            &f.admission,
            req.raw(),
            || ticks.next().unwrap(),
        )
        .unwrap();
    assert_eq!(
        Reply::decode_for(&req, &bytes).unwrap().outcome,
        Outcome::Rejected(Error::CommitUnknown)
    );
    assert!(matches!(f.snapshot(0,16),Outcome::Snapshot(s) if s.info.tail==2));
    assert!(matches!(f.request(&req),Outcome::Session(info) if info.tail==2));
}

#[test]
fn aggregate_reply_limit_rejects_without_partial_plaintext() {
    let mut f = Fixture::new();
    f.writer();
    for index in 0..4 {
        assert!(matches!(
            f.append(&format!("event-{index}"), index, vec![b'x'; 32 * 1024]),
            Outcome::Session(_)
        ));
    }
    assert_eq!(f.snapshot(0, 16), Outcome::Rejected(Error::Limit));
    assert!(matches!(f.snapshot(0,2),Outcome::Snapshot(s) if s.events.len()==2));
}

#[test]
fn list_is_bounded_to_host_nominations_and_clock_regression_denies() {
    let mut f = Fixture::new();
    f.create();
    assert!(
        matches!(f.send("list",Action::List),Outcome::Sessions(s) if s.len()==1 && s[0].session_id==SESSION)
    );
    let req = Request::new("backward", Action::List).unwrap();
    let bytes = f
        .host
        .dispatch(&mut f.runtime, &f.connection, &f.admission, req.raw(), || 0)
        .unwrap();
    assert_eq!(
        Reply::decode_for(&req, &bytes).unwrap().outcome,
        Outcome::Rejected(Error::Denied)
    );
    f.runtime.disconnect(&f.connection).unwrap();
    assert_eq!(f.snapshot(0, 16), Outcome::Rejected(Error::Denied));
}

#[test]
fn fork_retains_its_sealed_baseline_when_parent_advances_and_reopens() {
    let mut f = Fixture::new();
    f.writer();
    f.append("a", 0, b"parent event".to_vec());
    f.send(
        "seal-first",
        Action::Checkpoint {
            session_id: SESSION.into(),
            epoch: 1,
            expected_tail: 1,
            state: b"fork baseline".to_vec(),
        },
    );
    assert!(
        matches!(f.send("fork",Action::Create{session_id:"child".into(),parent:Some(SESSION.into()),parent_tail:1}),Outcome::Session(info) if info.checkpoint_sealed && info.parent_checkpoint_sha256==Some(hash(b"fork baseline")))
    );
    f.append("parent-next", 1, vec![]);
    f.send(
        "seal-second",
        Action::Checkpoint {
            session_id: SESSION.into(),
            epoch: 1,
            expected_tail: 2,
            state: b"new parent checkpoint".to_vec(),
        },
    );
    let path = f.temp.path().join("original.sqlite");
    drop(f.runtime);
    let mut runtime =
        HostRuntime::new(Store::open_existing(&path, EventBudget::default()).unwrap()).unwrap();
    let connection = runtime.connect().unwrap();
    let host = SessionExecHost::new(&mut runtime).unwrap();
    let admission = host
        .admit(
            &runtime,
            &connection,
            caps(),
            caps(),
            vec!["child".into()],
            "domain".into(),
            100,
            0,
        )
        .unwrap();
    let req = Request::new(
        "child-recovery",
        Action::Snapshot {
            session_id: "child".into(),
            after: 0,
            limit: 16,
        },
    )
    .unwrap();
    let bytes = host
        .dispatch(&mut runtime, &connection, &admission, req.raw(), || 1)
        .unwrap();
    assert!(
        matches!(Reply::decode_for(&req,&bytes).unwrap().outcome,Outcome::Snapshot(s) if s.info.parent_tail==1 && s.info.checkpoint_tail==0 && s.info.tail==0 && s.checkpoint==b"fork baseline" && s.info.parent_checkpoint_sha256==Some(hash(b"fork baseline")))
    );
}

#[test]
fn replacing_original_store_with_same_persisted_identity_does_not_transfer_authority() {
    let mut f = Fixture::new();
    f.writer();
    f.append("secret", 0, b"original session data".to_vec());
    let replacement = Store::open_existing(
        &f.temp.path().join("original.sqlite"),
        EventBudget::default(),
    )
    .unwrap();
    assert_eq!(
        replacement.tls_store_identity().unwrap(),
        f.runtime.store_local().tls_store_identity().unwrap()
    );
    let original = std::mem::replace(f.runtime.store_local_mut(), replacement);
    assert_eq!(f.snapshot(0, 16), Outcome::Rejected(Error::Denied));
    let _replacement = std::mem::replace(f.runtime.store_local_mut(), original);
    assert!(
        matches!(f.snapshot(0,16),Outcome::Snapshot(s) if s.events[0].event.body==b"original session data")
    );
}

#[test]
fn corrupt_canonical_state_overflow_and_parent_hash_return_rejection_without_panic() {
    use prost::Message;
    #[derive(Clone, PartialEq, Message)]
    struct Corrupt {
        #[prost(string, tag = "1")]
        id: String,
        #[prost(string, optional, tag = "2")]
        parent: Option<String>,
        #[prost(uint64, tag = "5")]
        tail: u64,
        #[prost(uint64, tag = "6")]
        floor: u64,
        #[prost(uint64, tag = "8")]
        revision: u64,
        #[prost(uint64, tag = "9")]
        checkpoint_tail: u64,
        #[prost(bool, tag = "16")]
        checkpoint_sealed: bool,
        #[prost(bytes = "vec", tag = "17")]
        parent_checkpoint: Vec<u8>,
    }
    for bad_parent in [false, true] {
        let mut f = Fixture::new();
        let corrupt = if bad_parent {
            Corrupt {
                id: SESSION.into(),
                parent: Some("parent".into()),
                tail: 0,
                floor: 1,
                revision: 1,
                checkpoint_tail: 0,
                checkpoint_sealed: false,
                parent_checkpoint: vec![1],
            }
        } else {
            Corrupt {
                id: SESSION.into(),
                parent: None,
                tail: u64::MAX,
                floor: 2,
                revision: 1,
                checkpoint_tail: u64::MAX,
                checkpoint_sealed: true,
                parent_checkpoint: vec![],
            }
        };
        let row = morrow_core::agent_ledger::Record::new(
            hash(b"morrow/agent-session-exec-v1/session/state/1"),
            SESSION,
            1,
            corrupt.encode_to_vec(),
        )
        .unwrap();
        f.runtime
            .store_local_mut()
            .compare_exchange_agent_ledger_local(&row, 0)
            .unwrap();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f.snapshot(0, 16)));
        assert_eq!(outcome.unwrap(), Outcome::Rejected(Error::Invalid));
    }
}
