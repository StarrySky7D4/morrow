use capnp::{message::Builder, serialize};
use morrow_agent_catalog_admin_v1::{agent_catalog_capnp as wire, *};

fn revisions() -> Revisions {
    Revisions {
        catalog: u64::MAX,
        manager: (1u64 << 53) + 19,
    }
}
fn approval() -> Approval {
    Approval {
        session_bits: 31,
        process_bits: 127,
        sessions: vec!["session:a".into(), "session:z".into()],
        domain: "local:test".into(),
    }
}
fn review() -> Review {
    Review {
        id: "codex.fixture".into(),
        version: "0.1.0".into(),
        full_sha256: [2; 32],
        base_sha256: [3; 32],
        session_schema: [4; 32],
        process_schema: [5; 32],
        session_bits: 31,
        process_bits: 127,
        sessions: approval().sessions,
        domain: approval().domain,
    }
}
fn request(action: Action) -> Request {
    Request {
        id: [1; 16],
        action,
    }
}
fn actions() -> Vec<Action> {
    let r = revisions();
    vec![
        Action::State,
        Action::Inspect {
            path: "C:/插件/完整包.mrowasp".into(),
        },
        Action::Install {
            path: "C:/插件/完整包.mrowasp".into(),
            full_sha256: [2; 32],
            revisions: r,
        },
        Action::BaseSelect {
            full_sha256: [2; 32],
            revisions: r,
        },
        Action::BaseEnable {
            id: "codex.fixture".into(),
            full_sha256: [2; 32],
            enabled: true,
            revisions: r,
        },
        Action::WrapperSelect {
            full_sha256: [2; 32],
            revisions: r,
        },
        Action::Approve {
            id: "codex.fixture".into(),
            full_sha256: [2; 32],
            approval: approval(),
            revisions: r,
        },
        Action::WrapperEnable {
            id: "codex.fixture".into(),
            full_sha256: [2; 32],
            enabled: false,
            revisions: r,
        },
        Action::Remove {
            id: "codex.fixture".into(),
            full_sha256: [2; 32],
            revisions: r,
        },
        Action::Page {
            after: None,
            limit: 16,
            revisions: r,
        },
    ]
}
fn frame(b: &Builder<capnp::message::HeapAllocator>) -> Vec<u8> {
    let mut v = MAGIC.to_vec();
    v.extend(serialize::write_message_to_words(b));
    v
}
fn raw_state(version: u16, digest: [u8; 32], unused_path: bool) -> Vec<u8> {
    let mut b = Builder::new_default();
    {
        let mut w = b.init_root::<wire::request::Builder>();
        w.set_version(version);
        w.set_digest(&digest);
        w.set_id(&[1; 16]);
        if unused_path {
            w.set_path("forbidden");
        }
    }
    frame(&b)
}
struct Mock {
    calls: usize,
    outcome: Outcome,
}
impl Target for Mock {
    fn execute(&mut self, _: &Request) -> Outcome {
        self.calls += 1;
        self.outcome.clone()
    }
}

#[test]
fn all_actions_roundtrip_preserve_large_revisions_and_explicit_mutations() {
    for action in actions() {
        let r = request(action);
        let b = r.encode().unwrap();
        assert_eq!(Request::decode(&b).unwrap(), r);
        assert_eq!(r.is_mutation(), (2..=8).contains(&r.action.tag()));
    }
}
#[test]
fn all_failure_statuses_are_correlated_and_have_no_body() {
    let r = request(Action::State);
    for s in [
        Status::Invalid,
        Status::Conflict,
        Status::Denied,
        Status::NotFound,
        Status::Limit,
        Status::Storage,
        Status::Unknown,
        Status::Busy,
        Status::RecoveryRequired,
        Status::OwnerUnavailable,
        Status::Unsupported,
    ] {
        let reply = Reply::new(&r, Outcome::failure(s, revisions())).unwrap();
        let b = reply.encode_for(&r).unwrap();
        assert_eq!(Reply::decode_for(&r, &b).unwrap(), reply);
    }
}
#[test]
fn invalid_frames_never_reach_target() {
    let good = request(Action::State).encode().unwrap();
    let mut bad_magic = good.clone();
    bad_magic[0] ^= 1;
    let mut trailing = good.clone();
    trailing.extend([0; 8]);
    let mut unknown_action = good.clone();
    unknown_action[26] = 255;
    unknown_action[27] = 255;
    let cases = [
        bad_magic,
        trailing,
        unknown_action,
        raw_state(2, SCHEMA_DIGEST, false),
        raw_state(1, [9; 32], false),
        raw_state(1, SCHEMA_DIGEST, true),
        vec![0; MAX_FRAME_BYTES + 1],
    ];
    let mut mock = Mock {
        calls: 0,
        outcome: Outcome::ok(revisions(), Body::None),
    };
    for bad in cases {
        assert!(respond(&mut mock, &bad).is_err());
    }
    for end in 0..good.len() {
        assert!(respond(&mut mock, &good[..end]).is_err());
    }
    assert_eq!(mock.calls, 0);
}
#[test]
fn unused_action_data_cannot_smuggle_an_effect_and_can_be_rejected() {
    let bad = raw_state(1, SCHEMA_DIGEST, true);
    assert!(Request::decode(&bad).is_err());
    let rejection = reject_frame(&bad, revisions()).unwrap();
    let mut slice = &rejection[8..];
    let m = serialize::read_message_from_flat_slice(&mut slice, Default::default()).unwrap();
    let r = m.get_root::<wire::reply::Reader>().unwrap();
    assert_eq!(r.get_status().unwrap(), wire::Status::Invalid);
    assert_eq!(r.get_id().unwrap(), [1; 16]);
    assert_eq!(r.get_request_sha256().unwrap(), hash(&bad));
    assert_eq!(r.get_revisions().unwrap().get_catalog(), u64::MAX);
}
#[test]
fn approval_limits_order_unknown_bits_and_path_limits_are_enforced() {
    let mut r = request(Action::Approve {
        id: "codex.fixture".into(),
        full_sha256: [2; 32],
        approval: approval(),
        revisions: revisions(),
    });
    if let Action::Approve { approval: a, .. } = &mut r.action {
        a.sessions.swap(0, 1);
    }
    assert!(r.encode().is_err());
    if let Action::Approve { approval: a, .. } = &mut r.action {
        a.sessions = vec!["a".into(); 17];
    }
    assert!(r.encode().is_err());
    if let Action::Approve { approval: a, .. } = &mut r.action {
        *a = approval();
        a.process_bits = 128;
    }
    assert!(r.encode().is_err());
    if let Action::Approve { approval: a, .. } = &mut r.action {
        *a = approval();
        a.session_bits = 0;
    }
    assert!(r.encode().is_err());
    assert!(
        request(Action::Inspect {
            path: "x".repeat(MAX_PATH_BYTES)
        })
        .encode()
        .is_ok()
    );
    assert!(
        request(Action::Inspect {
            path: "x".repeat(MAX_PATH_BYTES + 1)
        })
        .encode()
        .is_err()
    );
    assert!(
        Request {
            id: [0; 16],
            action: Action::State
        }
        .encode()
        .is_err()
    );
    for limit in [0, 17] {
        assert!(
            request(Action::Page {
                after: None,
                limit,
                revisions: revisions()
            })
            .encode()
            .is_err()
        );
    }
    assert!(
        request(Action::Page {
            after: Some("A".repeat(64)),
            limit: 1,
            revisions: revisions()
        })
        .encode()
        .is_err()
    );
}
#[test]
fn reply_association_and_kind_mismatch_are_rejected() {
    let r = request(Action::State);
    let reply = Reply::new(&r, Outcome::ok(revisions(), Body::None)).unwrap();
    let bytes = reply.encode_for(&r).unwrap();
    let mut other = r.clone();
    other.id = [2; 16];
    assert!(Reply::decode_for(&other, &bytes).is_err());
    other = r.clone();
    other.action = Action::BaseSelect {
        full_sha256: [2; 32],
        revisions: revisions(),
    };
    assert!(Reply::decode_for(&other, &bytes).is_err());
    let mut wrong = reply.clone();
    wrong.request_sha256 = [0; 32];
    assert!(wrong.encode_for(&r).is_err());
    assert!(Reply::new(&r, Outcome::ok(revisions(), Body::Review(review()))).is_err());
    assert!(
        Reply::new(
            &r,
            Outcome {
                status: Status::Denied,
                revisions: revisions(),
                body: Body::Review(review())
            }
        )
        .is_err()
    );
    let mut trailing = bytes.clone();
    trailing.extend([0; 8]);
    assert!(Reply::decode_for(&r, &trailing).is_err());
}
fn entry() -> Entry {
    Entry {
        review: review(),
        selected: true,
        enabled: true,
        approval: Some(approval()),
        base_selected: true,
        base_enabled: false,
    }
}
#[test]
fn page_order_budget_cursor_approval_and_revision_are_enforced() {
    let r = request(Action::Page {
        after: None,
        limit: 1,
        revisions: revisions(),
    });
    let body = Body::Page {
        entries: vec![entry()],
        next: Some("02".repeat(32)),
    };
    let reply = Reply::new(&r, Outcome::ok(revisions(), body)).unwrap();
    let b = reply.encode_for(&r).unwrap();
    assert_eq!(Reply::decode_for(&r, &b).unwrap(), reply);
    assert!(
        Reply::new(
            &r,
            Outcome::ok(
                Revisions::default(),
                Body::Page {
                    entries: vec![],
                    next: None
                }
            )
        )
        .is_err()
    );
    assert!(
        Reply::new(
            &r,
            Outcome::ok(
                revisions(),
                Body::Page {
                    entries: vec![entry(), entry()],
                    next: None
                }
            )
        )
        .is_err()
    );
    assert!(
        Reply::new(
            &r,
            Outcome::ok(
                revisions(),
                Body::Page {
                    entries: vec![],
                    next: Some("02".repeat(32))
                }
            )
        )
        .is_err()
    );
    assert!(
        Reply::new(
            &r,
            Outcome::ok(
                revisions(),
                Body::Page {
                    entries: vec![entry()],
                    next: Some("03".repeat(32))
                }
            )
        )
        .is_err()
    );
    let mut e = entry();
    e.approval.as_mut().unwrap().domain = "different".into();
    assert!(
        Reply::new(
            &r,
            Outcome::ok(
                revisions(),
                Body::Page {
                    entries: vec![e],
                    next: None
                }
            )
        )
        .is_err()
    );
    let r = request(Action::Page {
        after: Some("02".repeat(32)),
        limit: 16,
        revisions: revisions(),
    });
    assert!(
        Reply::new(
            &r,
            Outcome::ok(
                revisions(),
                Body::Page {
                    entries: vec![entry()],
                    next: None
                }
            )
        )
        .is_err()
    );
}
#[test]
fn target_runs_once_unknown_is_not_retried_and_bad_mutation_result_is_unknown() {
    let r = request(Action::Remove {
        id: "codex.fixture".into(),
        full_sha256: [2; 32],
        revisions: revisions(),
    });
    let mut mock = Mock {
        calls: 0,
        outcome: Outcome::failure(Status::Unknown, revisions()),
    };
    let b = respond(&mut mock, &r.encode().unwrap()).unwrap();
    assert_eq!(
        Reply::decode_for(&r, &b).unwrap().outcome.status,
        Status::Unknown
    );
    assert_eq!(mock.calls, 1);
    mock.outcome = Outcome::ok(revisions(), Body::Review(review()));
    let b = respond(&mut mock, &r.encode().unwrap()).unwrap();
    assert_eq!(
        Reply::decode_for(&r, &b).unwrap().outcome.status,
        Status::Unknown
    );
    assert_eq!(mock.calls, 2);
    let read = request(Action::State);
    let b = respond(&mut mock, &read.encode().unwrap()).unwrap();
    assert_eq!(
        Reply::decode_for(&read, &b).unwrap().outcome.status,
        Status::Invalid
    );
    assert_eq!(mock.calls, 3);
}
#[test]
fn install_review_binds_full_archive_digest() {
    let r = request(Action::Install {
        path: "package".into(),
        full_sha256: [7; 32],
        revisions: revisions(),
    });
    assert!(Reply::new(&r, Outcome::ok(revisions(), Body::Review(review()))).is_err());
}
#[test]
fn maximal_legal_page_remains_single_segment_and_within_frame_budget() {
    let mut e = entry();
    e.review.id = "i".repeat(MAX_ID_BYTES);
    e.review.version = "v".repeat(MAX_VERSION_BYTES);
    e.review.sessions = (0..MAX_SCOPE)
        .map(|i| format!("{i:02}{}", "s".repeat(MAX_SCOPE_BYTES - 2)))
        .collect();
    e.review.domain = "d".repeat(MAX_SCOPE_BYTES);
    e.approval = Some(Approval {
        session_bits: 31,
        process_bits: 127,
        sessions: e.review.sessions.clone(),
        domain: e.review.domain.clone(),
    });
    let entries = (1..=MAX_PAGE)
        .map(|i| {
            let mut v = e.clone();
            v.review.full_sha256 = [i as u8; 32];
            v
        })
        .collect();
    let request = request(Action::Page {
        after: None,
        limit: MAX_PAGE as u16,
        revisions: revisions(),
    });
    let reply = Reply::new(
        &request,
        Outcome::ok(
            revisions(),
            Body::Page {
                entries,
                next: None,
            },
        ),
    )
    .unwrap();
    let bytes = reply.encode_for(&request).unwrap();
    assert!(bytes.len() <= MAX_FRAME_BYTES);
    assert_eq!(&bytes[8..12], &[0; 4]);
    assert_eq!(Reply::decode_for(&request, &bytes).unwrap(), reply);
}
