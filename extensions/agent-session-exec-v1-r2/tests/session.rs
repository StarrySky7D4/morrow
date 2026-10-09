use morrow_agent_session_exec_v1_r2::{
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
        retire: true,
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
        Self::new_with_budget(EventBudget::default())
    }
    fn new_with_budget(budget: EventBudget) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let mut runtime =
            HostRuntime::new(Store::open(&temp.path().join("original.sqlite"), budget).unwrap())
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
                1,
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
            1,
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
            1,
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
            1
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
            1,
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
            1,
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
    let mut f = Fixture::new();
    f.writer();
    f.append("a", 0, b"committed source".to_vec());
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
    let read = Capabilities {
        session_read: true,
        ..Capabilities::default()
    };
    let reader = f
        .host
        .admit(
            &f.runtime,
            &f.connection,
            read,
            read,
            vec![SESSION.into()],
            "domain".into(),
            1000,
            100,
        )
        .unwrap();
    let inspect = Request::new(
        "current-recovery",
        Action::Snapshot {
            session_id: SESSION.into(),
            after: 0,
            limit: 16,
        },
    )
    .unwrap();
    let bytes = f
        .host
        .dispatch(
            &mut f.runtime,
            &f.connection,
            &reader,
            inspect.raw(),
            || 100,
        )
        .unwrap();
    assert!(
        matches!(Reply::decode_for(&inspect,&bytes).unwrap().outcome,Outcome::Snapshot(snapshot) if snapshot.info.tail==2)
    );
    let bytes = f
        .host
        .dispatch(
            &mut f.runtime,
            &f.connection,
            &f.admission,
            req.raw(),
            || 100,
        )
        .unwrap();
    assert_eq!(
        Reply::decode_for(&req, &bytes).unwrap().outcome,
        Outcome::Rejected(Error::Denied)
    );
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
            1,
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
            hash(b"morrow/agent-session-exec-v1/session/state/2"),
            SESSION,
            1,
            corrupt.encode_to_vec(),
        )
        .unwrap();
        // Corrupt the original live Store through SQLite itself. Production
        // ledger APIs must keep denying legacy writes while its owner is live;
        // this fixture deliberately bypasses that API without replacing owner.
        let database = rusqlite::Connection::open(f.temp.path().join("original.sqlite")).unwrap();
        assert_eq!(
            database
                .execute(
                    "INSERT INTO agent_ledger(domain,id,revision,payload) VALUES(?1,?2,?3,?4)",
                    rusqlite::params![
                        row.domain().as_slice(),
                        row.id(),
                        row.revision() as i64,
                        row.container()
                    ],
                )
                .unwrap(),
            1
        );
        drop(database);
        assert_eq!(
            f.runtime
                .store_local()
                .load_agent_ledger_local(&row.domain(), row.id())
                .unwrap()
                .unwrap()
                .container(),
            row.container()
        );
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f.snapshot(0, 16)));
        assert!(matches!(
            outcome.unwrap(),
            Outcome::Rejected(Error::Invalid | Error::Limit)
        ));
    }
}

fn reviewed_retirement(f: &Fixture) -> Admission {
    let rights = Capabilities {
        retire: true,
        ..Capabilities::default()
    };
    f.host
        .admit(
            &f.runtime,
            &f.connection,
            rights,
            rights,
            vec![SESSION.into(), "child".into()],
            "synthetic-local".into(),
            100,
            1,
        )
        .unwrap()
}

#[test]
fn full_ordinary_receipts_still_checkpoint_archive_and_continue_with_exact_raw_retry() {
    use morrow_agent_session_exec_v1_r2::session::MAX_ORDINARY_SESSION_RECEIPTS;
    let mut f = Fixture::new();
    f.writer();
    for tail in 0..(MAX_ORDINARY_SESSION_RECEIPTS as u64 - 2) {
        assert!(matches!(
            f.append(&format!("ordinary-{tail}"), tail, vec![1]),
            Outcome::Session(_)
        ));
    }
    let tail = MAX_ORDINARY_SESSION_RECEIPTS as u64 - 2;
    assert_eq!(
        f.append("ordinary-overflow", tail, vec![2]),
        Outcome::Rejected(Error::Limit)
    );
    let seal = Request::new(
        "reserved-seal",
        Action::Checkpoint {
            session_id: SESSION.into(),
            epoch: 1,
            expected_tail: tail,
            state: vec![7; 32 * 1024],
        },
    )
    .unwrap();
    let first = f.request(&seal);
    assert!(
        matches!(&first,Outcome::Session(info) if info.checkpoint_sealed && info.checkpoint_tail==tail)
    );
    assert_eq!(f.request(&seal), first);
    assert_eq!(
        f.append("closing-append", tail, vec![3]),
        Outcome::Rejected(Error::Limit)
    );
    f.host
        .compact(
            &mut f.runtime,
            &f.connection,
            &f.admission,
            SESSION,
            tail,
            1,
        )
        .unwrap();
    assert_eq!(
        f.append("compaction-cannot-reopen", tail, vec![4]),
        Outcome::Rejected(Error::Limit)
    );
    assert!(
        matches!(f.send("continuation",Action::Create{session_id:"child".into(),parent:Some(SESSION.into()),parent_tail:tail}),Outcome::Session(info) if info.parent_checkpoint_sha256==Some(hash(&vec![7;32*1024])))
    );
    assert!(
        matches!(f.send("reserved-archive",Action::Archive{session_id:SESSION.into(),epoch:1,expected_tail:tail}),Outcome::Session(info) if info.archived)
    );
    assert!(
        matches!(f.snapshot(tail,1),Outcome::Snapshot(snapshot) if snapshot.info.archived && snapshot.checkpoint==vec![7;32*1024])
    );
}

#[test]
fn full_ordinary_receipts_recover_writer_after_restart_without_reviving_original_nonce() {
    use morrow_agent_session_exec_v1_r2::session::MAX_ORDINARY_SESSION_RECEIPTS;
    let mut f = Fixture::new();
    f.writer();
    for tail in 0..(MAX_ORDINARY_SESSION_RECEIPTS as u64 - 2) {
        assert!(matches!(
            f.append(&format!("ordinary-{tail}"), tail, vec![]),
            Outcome::Session(_)
        ));
    }
    let tail = MAX_ORDINARY_SESSION_RECEIPTS as u64 - 2;
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
            vec![SESSION.into(), "child".into()],
            "synthetic-local".into(),
            100,
            1,
        )
        .unwrap();
    let old = Request::new(
        "old-writer",
        Action::Append {
            session_id: SESSION.into(),
            epoch: 1,
            expected_tail: tail,
            events: vec![Event {
                event_id: "old".into(),
                body: vec![],
            }],
        },
    )
    .unwrap();
    let out = host
        .dispatch(&mut runtime, &connection, &f.admission, old.raw(), || 1)
        .unwrap();
    assert_eq!(
        Reply::decode_for(&old, &out).unwrap().outcome,
        Outcome::Rejected(Error::Denied)
    );
    let writer = Request::new(
        "reserved-recovery",
        Action::OpenWriter {
            session_id: SESSION.into(),
            expected_epoch: 1,
        },
    )
    .unwrap();
    let bytes = host
        .dispatch(&mut runtime, &connection, &admission, writer.raw(), || 1)
        .unwrap();
    assert!(
        matches!(Reply::decode_for(&writer,&bytes).unwrap().outcome,Outcome::Session(info) if info.epoch==2)
    );
    for (id, action) in [
        (
            "recovered-seal",
            Action::Checkpoint {
                session_id: SESSION.into(),
                epoch: 2,
                expected_tail: tail,
                state: b"recovered model".to_vec(),
            },
        ),
        (
            "recovered-archive",
            Action::Archive {
                session_id: SESSION.into(),
                epoch: 2,
                expected_tail: tail,
            },
        ),
    ] {
        let req = Request::new(id, action).unwrap();
        let bytes = host
            .dispatch(&mut runtime, &connection, &admission, req.raw(), || 1)
            .unwrap();
        assert!(matches!(
            Reply::decode_for(&req, &bytes).unwrap().outcome,
            Outcome::Session(_)
        ));
    }
    let bytes = host
        .dispatch(&mut runtime, &connection, &admission, writer.raw(), || 1)
        .unwrap();
    assert!(
        matches!(Reply::decode_for(&writer,&bytes).unwrap().outcome,Outcome::Session(info) if info.epoch==2)
    );
    let second = Request::new(
        "second-recovery",
        Action::OpenWriter {
            session_id: SESSION.into(),
            expected_epoch: 2,
        },
    )
    .unwrap();
    let bytes = host
        .dispatch(&mut runtime, &connection, &admission, second.raw(), || 1)
        .unwrap();
    assert_eq!(
        Reply::decode_for(&second, &bytes).unwrap().outcome,
        Outcome::Rejected(Error::Denied)
    );
}

#[test]
fn full_payload_has_already_charged_room_for_max_checkpoint_and_archive() {
    use morrow_agent_session_exec_v1_r2::session::{
        MAX_SESSION_STATE_BYTES, SESSION_CONTROL_RESERVE_BYTES,
    };
    let mut f = Fixture::new();
    f.writer();
    let mut tail = 0;
    loop {
        let out = f.append(&format!("large-{tail}"), tail, vec![9; 32 * 1024]);
        if matches!(out, Outcome::Session(_)) {
            tail += 1;
        } else {
            assert_eq!(out, Outcome::Rejected(Error::Limit));
            break;
        }
    }
    assert!(tail > 0 && tail < 32);
    let domain = hash(b"morrow/agent-session-exec-v1/session/state/2");
    let before = f
        .runtime
        .store_local()
        .load_agent_ledger_local(&domain, SESSION)
        .unwrap()
        .unwrap();
    assert!(before.payload().len() <= MAX_SESSION_STATE_BYTES);
    assert!(before.payload().len() > MAX_SESSION_STATE_BYTES - 2 * SESSION_CONTROL_RESERVE_BYTES);
    let seal = f.send(
        "payload-reserved-seal",
        Action::Checkpoint {
            session_id: SESSION.into(),
            epoch: 1,
            expected_tail: tail,
            state: vec![5; 32 * 1024],
        },
    );
    assert!(
        matches!(seal,Outcome::Session(info) if info.checkpoint_sha256==hash(&vec![5;32*1024]))
    );
    let after = f
        .runtime
        .store_local()
        .load_agent_ledger_local(&domain, SESSION)
        .unwrap()
        .unwrap();
    assert_eq!(after.payload().len(), before.payload().len());
    assert_eq!(after.retained_bytes(), before.retained_bytes());
    assert!(
        matches!(f.send("payload-reserved-archive",Action::Archive{session_id:SESSION.into(),epoch:1,expected_tail:tail}),Outcome::Session(info) if info.archived)
    );
    let archived = f
        .runtime
        .store_local()
        .load_agent_ledger_local(&domain, SESSION)
        .unwrap()
        .unwrap();
    assert_eq!(archived.retained_bytes(), before.retained_bytes());
}

#[test]
fn retired_session_requires_explicit_right_exact_review_and_inactive_writer() {
    let mut f = Fixture::new();
    f.writer();
    f.append("opaque-model", 0, b"model".to_vec());
    let retirer = reviewed_retirement(&f);
    let review = f
        .host
        .review_session_retirement(&f.runtime, &f.connection, &retirer, SESSION, 1)
        .unwrap();
    assert_eq!(
        f.host.retire_session(
            &mut f.runtime,
            &f.connection,
            &retirer,
            SESSION,
            review.info.revision,
            review.record_sha256,
            || 1
        ),
        Err(Error::Denied)
    );
    f.host.revoke(&f.admission).unwrap();
    let ordinary = Capabilities {
        session_read: true,
        session_write: true,
        ..Capabilities::default()
    };
    let not_retire = f
        .host
        .admit(
            &f.runtime,
            &f.connection,
            ordinary,
            ordinary,
            vec![SESSION.into()],
            "synthetic-local".into(),
            100,
            1,
        )
        .unwrap();
    assert_eq!(
        f.host.retire_session(
            &mut f.runtime,
            &f.connection,
            &not_retire,
            SESSION,
            review.info.revision,
            review.record_sha256,
            || 1
        ),
        Err(Error::Denied)
    );
    assert_eq!(
        f.host.retire_session(
            &mut f.runtime,
            &f.connection,
            &retirer,
            SESSION,
            review.info.revision + 1,
            review.record_sha256,
            || 1
        ),
        Err(Error::Conflict)
    );
    assert_eq!(
        f.host.retire_session(
            &mut f.runtime,
            &f.connection,
            &retirer,
            SESSION,
            review.info.revision,
            [7; 32],
            || 1
        ),
        Err(Error::Conflict)
    );
    assert_eq!(
        f.host.retire_session(
            &mut f.runtime,
            &f.connection,
            &retirer,
            SESSION,
            review.info.revision,
            review.record_sha256,
            || 1
        ),
        Ok(())
    );
    assert_eq!(
        f.host.retire_session(
            &mut f.runtime,
            &f.connection,
            &retirer,
            SESSION,
            review.info.revision,
            review.record_sha256,
            || 1
        ),
        Ok(())
    );
    assert!(
        f.runtime
            .store_local()
            .load_agent_ledger_local(
                &hash(b"morrow/agent-session-exec-v1/session/state/2"),
                SESSION
            )
            .unwrap()
            .is_none()
    );
    let fresh = f
        .host
        .admit(
            &f.runtime,
            &f.connection,
            caps(),
            caps(),
            vec![SESSION.into()],
            "synthetic-local".into(),
            100,
            1,
        )
        .unwrap();
    let create = Request::new(
        "old-identity-reuse",
        Action::Create {
            session_id: SESSION.into(),
            parent: None,
            parent_tail: 0,
        },
    )
    .unwrap();
    let bytes = f
        .host
        .dispatch(&mut f.runtime, &f.connection, &fresh, create.raw(), || 1)
        .unwrap();
    assert_eq!(
        Reply::decode_for(&create, &bytes).unwrap().outcome,
        Outcome::Rejected(Error::Conflict)
    );
}

#[test]
fn deleting_reviewed_parent_preserves_self_contained_child_checkpoint() {
    let mut f = Fixture::new();
    f.writer();
    f.append("parent-model", 0, b"input".to_vec());
    f.send(
        "parent-seal",
        Action::Checkpoint {
            session_id: SESSION.into(),
            epoch: 1,
            expected_tail: 1,
            state: b"inherited baseline".to_vec(),
        },
    );
    f.send(
        "child-fork",
        Action::Create {
            session_id: "child".into(),
            parent: Some(SESSION.into()),
            parent_tail: 1,
        },
    );
    let retirer = reviewed_retirement(&f);
    let review = f
        .host
        .review_session_retirement(&f.runtime, &f.connection, &retirer, SESSION, 1)
        .unwrap();
    f.host.revoke(&f.admission).unwrap();
    f.host
        .retire_session(
            &mut f.runtime,
            &f.connection,
            &retirer,
            SESSION,
            review.info.revision,
            review.record_sha256,
            || 1,
        )
        .unwrap();
    let reader = Capabilities {
        session_read: true,
        ..Capabilities::default()
    };
    let read = f
        .host
        .admit(
            &f.runtime,
            &f.connection,
            reader,
            reader,
            vec!["child".into()],
            "synthetic-local".into(),
            100,
            1,
        )
        .unwrap();
    let req = Request::new(
        "child-independent",
        Action::Snapshot {
            session_id: "child".into(),
            after: 0,
            limit: 1,
        },
    )
    .unwrap();
    let bytes = f
        .host
        .dispatch(&mut f.runtime, &f.connection, &read, req.raw(), || 1)
        .unwrap();
    assert!(
        matches!(Reply::decode_for(&req,&bytes).unwrap().outcome,Outcome::Snapshot(snapshot) if snapshot.checkpoint==b"inherited baseline" && snapshot.info.parent_checkpoint_sha256==Some(hash(b"inherited baseline")))
    );
}

#[test]
fn archive_is_history_and_never_physical_retirement_permission() {
    let mut f = Fixture::new();
    f.writer();
    f.send(
        "logical-archive",
        Action::Archive {
            session_id: SESSION.into(),
            epoch: 1,
            expected_tail: 0,
        },
    );
    let domain = hash(b"morrow/agent-session-exec-v1/session/state/2");
    assert!(
        f.runtime
            .store_local()
            .load_agent_ledger_local(&domain, SESSION)
            .unwrap()
            .is_some()
    );
    let request = Request::new(
        "archived-read",
        Action::Snapshot {
            session_id: SESSION.into(),
            after: 0,
            limit: 1,
        },
    )
    .unwrap();
    assert!(matches!(f.request(&request),Outcome::Snapshot(snapshot) if snapshot.info.archived));
}

#[test]
fn shared_quota_rejection_still_uses_already_charged_closing_space() {
    let mut f = Fixture::new_with_budget(EventBudget {
        max_count: 1024,
        max_bytes: 200 * 1024,
    });
    f.writer();
    let mut tail = 0;
    loop {
        let out = f.append(&format!("shared-{tail}"), tail, vec![1]);
        if matches!(out, Outcome::Session(_)) {
            tail += 1;
        } else {
            assert_eq!(out, Outcome::Rejected(Error::Limit));
            break;
        }
    }
    assert!(tail > 0 && tail < 123);
    let domain = hash(b"morrow/agent-session-exec-v1/session/state/2");
    let before = f
        .runtime
        .store_local()
        .load_agent_ledger_local(&domain, SESSION)
        .unwrap()
        .unwrap();
    assert!(
        matches!(f.send("shared-seal",Action::Checkpoint{session_id:SESSION.into(),epoch:1,expected_tail:tail,state:vec![3;32*1024]}),Outcome::Session(info) if info.checkpoint_tail==tail)
    );
    let after = f
        .runtime
        .store_local()
        .load_agent_ledger_local(&domain, SESSION)
        .unwrap()
        .unwrap();
    assert_eq!(before.retained_bytes(), after.retained_bytes());
    assert!(
        matches!(f.send("shared-archive",Action::Archive{session_id:SESSION.into(),epoch:1,expected_tail:tail}),Outcome::Session(info) if info.archived)
    );
}

#[test]
fn new_writer_control_reserve_cannot_be_consumed_twice_before_seal_and_archive() {
    use morrow_agent_session_exec_v1_r2::session::MAX_ORDINARY_SESSION_RECEIPTS;
    let mut f = Fixture::new();
    f.writer();
    for tail in 0..(MAX_ORDINARY_SESSION_RECEIPTS as u64 - 2) {
        assert!(matches!(
            f.append(&format!("ordinary-{tail}"), tail, vec![]),
            Outcome::Session(_)
        ));
    }
    let tail = MAX_ORDINARY_SESSION_RECEIPTS as u64 - 2;
    assert!(
        matches!(f.send("first-recovery",Action::OpenWriter{session_id:SESSION.into(),expected_epoch:1}),Outcome::Session(info) if info.epoch==2)
    );
    assert_eq!(
        f.send(
            "cannot-drain-reserve",
            Action::OpenWriter {
                session_id: SESSION.into(),
                expected_epoch: 2
            }
        ),
        Outcome::Rejected(Error::Limit)
    );
    assert!(matches!(
        f.send(
            "after-recovery-seal",
            Action::Checkpoint {
                session_id: SESSION.into(),
                epoch: 2,
                expected_tail: tail,
                state: b"final".to_vec()
            }
        ),
        Outcome::Session(_)
    ));
    assert!(
        matches!(f.send("after-recovery-archive",Action::Archive{session_id:SESSION.into(),epoch:2,expected_tail:tail}),Outcome::Session(info) if info.archived)
    );
}

#[test]
fn retained_tool_blocks_physical_session_delete_even_after_writer_revocation() {
    use morrow_agent_session_exec_v1_r2::Intent;
    let mut f = Fixture::new();
    f.writer();
    let intent = Intent {
        operation_id: "retained-tool".into(),
        artifact_sha256: hash(b"reviewed synthetic artifact"),
        program: "/trusted/tool".into(),
        argv: vec![],
        cwd: "/trusted/work".into(),
        env: vec![],
        input: vec![],
        execution_domain: "synthetic-local".into(),
        max_runtime_ms: 1000,
    };
    assert!(matches!(
        f.send(
            "tool-proposal",
            Action::Propose {
                session_id: SESSION.into(),
                intent
            }
        ),
        Outcome::Tool(_)
    ));
    let retirer = reviewed_retirement(&f);
    let review = f
        .host
        .review_session_retirement(&f.runtime, &f.connection, &retirer, SESSION, 1)
        .unwrap();
    f.host.revoke(&f.admission).unwrap();
    assert_eq!(
        f.host.retire_session(
            &mut f.runtime,
            &f.connection,
            &retirer,
            SESSION,
            review.info.revision,
            review.record_sha256,
            || 1
        ),
        Err(Error::Conflict)
    );
    assert!(
        f.runtime
            .store_local()
            .load_agent_ledger_local(
                &hash(b"morrow/agent-session-exec-v1/session/state/2"),
                SESSION
            )
            .unwrap()
            .is_some()
    );
}

#[test]
fn physical_retirement_lost_ack_is_unknown_and_fresh_review_cannot_restore_old_identity() {
    let mut f = Fixture::new();
    f.writer();
    let retirer = reviewed_retirement(&f);
    let review = f
        .host
        .review_session_retirement(&f.runtime, &f.connection, &retirer, SESSION, 1)
        .unwrap();
    f.host.revoke(&f.admission).unwrap();
    let mut ticks = [1, 1, 1, 100].into_iter();
    assert_eq!(
        f.host.retire_session(
            &mut f.runtime,
            &f.connection,
            &retirer,
            SESSION,
            review.info.revision,
            review.record_sha256,
            || ticks.next().unwrap()
        ),
        Err(Error::CommitUnknown)
    );
    assert!(
        f.runtime
            .store_local()
            .load_agent_ledger_local(
                &hash(b"morrow/agent-session-exec-v1/session/state/2"),
                SESSION
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(
        f.host.retire_session(
            &mut f.runtime,
            &f.connection,
            &retirer,
            SESSION,
            review.info.revision,
            review.record_sha256,
            || 100
        ),
        Err(Error::Denied)
    );
    let rights = Capabilities {
        retire: true,
        ..Capabilities::default()
    };
    let current = f
        .host
        .admit(
            &f.runtime,
            &f.connection,
            rights,
            rights,
            vec![SESSION.into()],
            "synthetic-local".into(),
            1000,
            100,
        )
        .unwrap();
    assert_eq!(
        f.host.retire_session(
            &mut f.runtime,
            &f.connection,
            &current,
            SESSION,
            review.info.revision,
            review.record_sha256,
            || 100
        ),
        Ok(())
    );
}

#[test]
fn explicit_generation_rollover_releases_tombstone_budget_but_rejects_original_raw_frame() {
    let mut f = Fixture::new();
    f.create();
    let original = Request::new(
        "create",
        Action::Create {
            session_id: SESSION.into(),
            parent: None,
            parent_tail: 0,
        },
    )
    .unwrap();
    let retirer = reviewed_retirement(&f);
    let review = f
        .host
        .review_session_retirement(&f.runtime, &f.connection, &retirer, SESSION, 1)
        .unwrap();
    f.host.revoke(&f.admission).unwrap();
    f.host
        .retire_session(
            &mut f.runtime,
            &f.connection,
            &retirer,
            SESSION,
            review.info.revision,
            review.record_sha256,
            || 1,
        )
        .unwrap();
    assert_eq!(f.host.rollover_generation(&mut f.runtime, 1), Ok(2));
    let host = SessionExecHost::new(&mut f.runtime).unwrap();
    let fresh = host
        .admit(
            &f.runtime,
            &f.connection,
            caps(),
            caps(),
            vec![SESSION.into()],
            "synthetic-local".into(),
            100,
            1,
        )
        .unwrap();
    let bytes = host
        .dispatch(&mut f.runtime, &f.connection, &fresh, original.raw(), || 1)
        .unwrap();
    assert_eq!(
        Reply::decode_for(&original, &bytes).unwrap().outcome,
        Outcome::Rejected(Error::Conflict)
    );
    let current = Request::new_for_generation(
        "create",
        2,
        Action::Create {
            session_id: SESSION.into(),
            parent: None,
            parent_tail: 0,
        },
    )
    .unwrap();
    let bytes = host
        .dispatch(&mut f.runtime, &f.connection, &fresh, current.raw(), || 1)
        .unwrap();
    assert!(
        matches!(Reply::decode_for(&current,&bytes).unwrap().outcome,Outcome::Session(info) if info.tail==0)
    );
    let bytes = host
        .dispatch(
            &mut f.runtime,
            &f.connection,
            &f.admission,
            current.raw(),
            || 1,
        )
        .unwrap();
    assert_eq!(
        Reply::decode_for(&current, &bytes).unwrap().outcome,
        Outcome::Rejected(Error::Denied)
    );
}

// Trusted retention regressions use the original Fixture and public ledger reads.
// The image comparison checks committed state; it does not decode private state.
fn retention_ledger_image(f: &Fixture) -> (u64, Vec<u8>) {
    let row = f.runtime.store_local()
        .load_agent_ledger_local(&hash(b"morrow/agent-session-exec-v1/session/state/2"), SESSION)
        .unwrap().unwrap();
    (row.revision(), row.payload().to_vec())
}

#[test]
fn compact_retains_exact_mutation_receipts_without_restoring_pruned_events() {
    let mut f = Fixture::new();
    f.writer();
    let append = Request::new("retained-append", Action::Append {
        session_id: SESSION.into(), epoch: 1, expected_tail: 0,
        events: vec![Event { event_id: "retained-event".into(), body: b"first".to_vec() }],
    }).unwrap();
    let append_reply = f.request(&append);
    assert!(matches!(&append_reply, Outcome::Session(info) if info.tail == 1));
    assert!(matches!(f.append("second", 1, b"second".to_vec()), Outcome::Session(info) if info.tail == 2));
    let seal = Request::new("retained-seal", Action::Checkpoint {
        session_id: SESSION.into(), epoch: 1, expected_tail: 2,
        state: b"retained checkpoint".to_vec(),
    }).unwrap();
    let seal_reply = f.request(&seal);
    assert!(matches!(&seal_reply, Outcome::Session(info) if info.checkpoint_sealed && info.checkpoint_tail == 2));
    let compacted = f.host.compact(&mut f.runtime, &f.connection, &f.admission, SESSION, 2, 1).unwrap();
    assert_eq!((compacted.floor, compacted.tail), (3, 2));
    let image = retention_ledger_image(&f);
    assert_eq!(f.request(&append), append_reply);
    assert_eq!(f.request(&seal), seal_reply);
    assert_eq!(retention_ledger_image(&f), image);
    let changed = Request::new("retained-append", Action::Append {
        session_id: SESSION.into(), epoch: 1, expected_tail: 0,
        events: vec![Event { event_id: "retained-event".into(), body: b"changed".to_vec() }],
    }).unwrap();
    assert_eq!(f.request(&changed), Outcome::Rejected(Error::Conflict));
    assert_eq!(retention_ledger_image(&f), image);
    for (after, gap) in [(0, true), (1, true), (2, false)] {
        match f.snapshot(after, 16) {
            Outcome::Snapshot(snapshot) => {
                assert_eq!(snapshot.info, compacted);
                assert_eq!(snapshot.gap, gap);
                assert!(snapshot.events.is_empty());
                assert_eq!(snapshot.checkpoint, b"retained checkpoint");
            }
            other => panic!("expected compacted snapshot: {other:?}"),
        }
    }
    assert!(matches!(f.append("after-retention", 2, b"third".to_vec()), Outcome::Session(info) if info.tail == 3));
    for (after, gap) in [(1, true), (2, false)] {
        match f.snapshot(after, 16) {
            Outcome::Snapshot(snapshot) => {
                assert_eq!((snapshot.info.floor, snapshot.info.tail), (3, 3));
                assert_eq!(snapshot.gap, gap);
                assert_eq!(snapshot.events.len(), 1);
                assert_eq!(snapshot.events[0].sequence, 3);
                assert_eq!(snapshot.events[0].event.event_id, "after-retention");
                assert_eq!(snapshot.events[0].event.body, b"third");
            }
            other => panic!("expected continued snapshot: {other:?}"),
        }
    }
}

#[test]
fn compact_rejects_unsealed_wrong_tail_and_foreign_or_replaced_writer_without_commit() {
    let mut f = Fixture::new();
    f.writer();
    assert!(matches!(f.append("before-fence", 0, b"event".to_vec()), Outcome::Session(info) if info.tail == 1));
    let unsealed = retention_ledger_image(&f);
    assert_eq!(f.host.compact(&mut f.runtime, &f.connection, &f.admission, SESSION, 1, 1), Err(Error::Conflict));
    assert_eq!(retention_ledger_image(&f), unsealed);
    assert!(matches!(f.send("fence-seal", Action::Checkpoint {
        session_id: SESSION.into(), epoch: 1, expected_tail: 1, state: b"sealed".to_vec(),
    }), Outcome::Session(info) if info.checkpoint_sealed));
    let sealed = retention_ledger_image(&f);
    assert_eq!(f.host.compact(&mut f.runtime, &f.connection, &f.admission, SESSION, 0, 1), Err(Error::Conflict));
    assert_eq!(retention_ledger_image(&f), sealed);
    let foreign = f.runtime.connect().unwrap();
    assert_eq!(f.host.compact(&mut f.runtime, &foreign, &f.admission, SESSION, 1, 1), Err(Error::Denied));
    assert_eq!(retention_ledger_image(&f), sealed);
    let other = f.host.admit(&f.runtime, &f.connection, caps(), caps(),
        vec![SESSION.into()], "synthetic-local".into(), 100, 1).unwrap();
    assert_eq!(f.host.compact(&mut f.runtime, &f.connection, &other, SESSION, 1, 1), Err(Error::Denied));
    assert_eq!(retention_ledger_image(&f), sealed);
    let handoff = Request::new("retention-handoff", Action::OpenWriter {
        session_id: SESSION.into(), expected_epoch: 1,
    }).unwrap();
    let bytes = f.host.dispatch(&mut f.runtime, &f.connection, &other, handoff.raw(), || 1).unwrap();
    assert!(matches!(Reply::decode_for(&handoff, &bytes).unwrap().outcome, Outcome::Session(info) if info.epoch == 2));
    let replaced = retention_ledger_image(&f);
    assert_eq!(f.host.compact(&mut f.runtime, &f.connection, &f.admission, SESSION, 1, 1), Err(Error::Denied));
    assert_eq!(f.append("stale-after-handoff", 1, b"must not commit".to_vec()), Outcome::Rejected(Error::Conflict));
    assert_eq!(retention_ledger_image(&f), replaced);
    let info = f.host.compact(&mut f.runtime, &f.connection, &other, SESSION, 1, 1).unwrap();
    assert_eq!((info.epoch, info.floor, info.tail), (2, 2, 1));
}

#[test]
fn fork_from_compacted_parent_keeps_parent_digest_when_child_checkpoint_changes() {
    let mut f = Fixture::new();
    f.writer();
    assert!(matches!(f.append("parent-event", 0, b"parent".to_vec()), Outcome::Session(info) if info.tail == 1));
    let baseline = b"fork baseline\0\xff";
    assert!(matches!(f.send("parent-checkpoint", Action::Checkpoint {
        session_id: SESSION.into(), epoch: 1, expected_tail: 1, state: baseline.to_vec(),
    }), Outcome::Session(info) if info.checkpoint_sha256 == hash(baseline)));
    f.host.compact(&mut f.runtime, &f.connection, &f.admission, SESSION, 1, 1).unwrap();
    assert!(matches!(f.send("compacted-fork", Action::Create {
        session_id: "child".into(), parent: Some(SESSION.into()), parent_tail: 1,
    }), Outcome::Session(info) if info.floor == 1 && info.tail == 0 && info.checkpoint_tail == 0
        && info.checkpoint_sha256 == hash(baseline) && info.parent_checkpoint_sha256 == Some(hash(baseline))));
    assert!(matches!(f.send("child-writer", Action::OpenWriter {
        session_id: "child".into(), expected_epoch: 0,
    }), Outcome::Session(info) if info.epoch == 1));
    assert!(matches!(f.send("child-event", Action::Append {
        session_id: "child".into(), epoch: 1, expected_tail: 0,
        events: vec![Event { event_id: "child-local".into(), body: b"child".to_vec() }],
    }), Outcome::Session(info) if info.tail == 1));
    let child_checkpoint = b"child independent checkpoint";
    assert!(matches!(f.send("child-checkpoint", Action::Checkpoint {
        session_id: "child".into(), epoch: 1, expected_tail: 1, state: child_checkpoint.to_vec(),
    }), Outcome::Session(info) if info.checkpoint_sha256 == hash(child_checkpoint)
        && info.parent_checkpoint_sha256 == Some(hash(baseline))));
    let child_info = f.host.compact(&mut f.runtime, &f.connection, &f.admission, "child", 1, 1).unwrap();
    assert!(matches!(f.append("parent-advance", 1, b"later".to_vec()), Outcome::Session(info) if info.tail == 2));
    assert!(matches!(f.send("parent-new-checkpoint", Action::Checkpoint {
        session_id: SESSION.into(), epoch: 1, expected_tail: 2, state: b"new parent baseline".to_vec(),
    }), Outcome::Session(info) if info.checkpoint_sha256 == hash(b"new parent baseline")));
    f.host.compact(&mut f.runtime, &f.connection, &f.admission, SESSION, 2, 1).unwrap();
    match f.send("child-final-snapshot", Action::Snapshot { session_id: "child".into(), after: 0, limit: 16 }) {
        Outcome::Snapshot(snapshot) => {
            assert_eq!(snapshot.info, child_info);
            assert_eq!((snapshot.info.floor, snapshot.info.tail, snapshot.info.checkpoint_tail), (2, 1, 1));
            assert_eq!(snapshot.info.parent.as_deref(), Some(SESSION));
            assert_eq!(snapshot.info.parent_tail, 1);
            assert_eq!(snapshot.info.parent_checkpoint_sha256, Some(hash(baseline)));
            assert_eq!(snapshot.info.checkpoint_sha256, hash(child_checkpoint));
            assert_eq!(snapshot.checkpoint, child_checkpoint);
            assert!(snapshot.gap);
            assert!(snapshot.events.is_empty());
        }
        other => panic!("expected independent child snapshot: {other:?}"),
    }
}
