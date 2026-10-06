//! Real original HostRuntime/Store integration on disposable ordinary databases.
//! This is bounded host-adapter evidence, not protected Session, authenticated
//! native transport, external-send permission or production Codex qualification.
use morrow_agent_content_v1::host::{ContentHost, ProposalPhase};
use morrow_agent_content_v1::{Action, ContentRef, Outcome, Reply, Request};
use morrow_core::{
    content::CardRecord,
    content_change::ContentChange,
    dispatch::{Connection, HostRuntime},
    lifecycle::GrantKind,
    response::{Failure, Outcome as CoreOutcome, Response},
    runtime::Command,
    store::{EventBudget, Store},
    transaction::Lookup,
};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const CARD: &str = "content-card";
const SIBLING: &str = "sibling-card";
const ORIGINAL: &[u8] = b"original\0body\xff";
const EDITED: &[u8] = b"new bounded body\0\xfe";
const EXPIRES: u64 = 100;

struct Fixture {
    _temp: tempfile::TempDir,
    path: PathBuf,
    host: HostRuntime,
    connection: Connection,
    content: ContentHost,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("ordinary-synthetic.sqlite");
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        for (card, operation) in [(CARD, "seed-content"), (SIBLING, "seed-sibling")] {
            let record =
                CardRecord::new(card, "org.example.note", 1, "Original", ORIGINAL.to_vec())
                    .unwrap();
            store.create_local(operation, &record).unwrap();
        }
        let mut host = HostRuntime::new(store).unwrap();
        let mut connection = host.connect().unwrap();
        for card in [CARD, SIBLING] {
            for kind in [
                GrantKind::ReadSummary,
                GrantKind::ReadContent,
                GrantKind::EditContent,
                GrantKind::QueryOperation,
            ] {
                host.grant(&mut connection, kind, card, EXPIRES, 0).unwrap();
            }
        }
        // SIBLING is granted in the original connection but is deliberately
        // excluded from the independent host nomination.
        let content = ContentHost::new(&host, &connection, vec![CARD.into()]).unwrap();
        Self {
            _temp: temp,
            path,
            host,
            connection,
            content,
        }
    }

    fn reference(&self) -> ContentRef {
        let record = self.host.store_local().card(CARD).unwrap().unwrap();
        let body = record.body();
        ContentRef {
            card_id: CARD.into(),
            revision: record.summary().revision,
            total_length: body.len() as u64,
            body_sha256: Sha256::digest(&body).into(),
        }
    }

    fn dispatch(&mut self, request: &Request, now: u64) -> Outcome {
        let wire = self
            .content
            .dispatch(
                &mut self.host,
                &self.connection,
                &request.encode().unwrap(),
                || now,
            )
            .unwrap();
        Reply::decode_for(&wire, request).unwrap().outcome
    }

    fn propose(&mut self, operation: &str) -> Request {
        let request = proposal_request(operation, self.reference(), EDITED);
        match self.dispatch(&request, 1) {
            Outcome::Proposed {
                operation_id,
                proposal_sha256,
            } => {
                assert_eq!(operation_id, operation);
                assert_eq!(proposal_sha256, request.digest());
            }
            other => panic!("expected a registered proposal: {other:?}"),
        }
        request
    }

    fn body(&self) -> Vec<u8> {
        self.host.store_local().card(CARD).unwrap().unwrap().body()
    }

    fn revision(&self) -> u64 {
        self.host
            .store_local()
            .card(CARD)
            .unwrap()
            .unwrap()
            .summary()
            .revision
    }

    fn approve(&mut self, request: &Request, operation: &str, now: u64) {
        self.content
            .approve(
                &self.host,
                &self.connection,
                operation,
                request.digest(),
                now,
            )
            .unwrap();
    }
}

fn proposal_request(operation: &str, reference: ContentRef, body: &[u8]) -> Request {
    let command = Command::EditContent(ContentChange {
        operation_id: operation.into(),
        card_id: reference.card_id.clone(),
        expected_revision: reference.revision,
        title: "Edited".into(),
        body: body.to_vec(),
        preview_text: "bounded preview".into(),
        attachments: None,
    })
    .encode()
    .unwrap();
    Request::new(
        format!("proposal-{operation}"),
        Action::ProposeMutation { reference, command },
    )
    .unwrap()
}

fn rejected(outcome: Outcome) {
    assert!(
        matches!(outcome, Outcome::Rejected { .. }),
        "expected refusal, got {outcome:?}"
    );
}

fn contains_plaintext(wire: &[u8], plaintext: &[u8]) -> bool {
    wire.windows(plaintext.len()).any(|part| part == plaintext)
}

fn summary_fixture(
    count: usize,
    preview: &str,
) -> (
    tempfile::TempDir,
    HostRuntime,
    Connection,
    ContentHost,
    Vec<String>,
) {
    let temp = tempfile::tempdir().unwrap();
    let mut store =
        Store::open(&temp.path().join("summary.sqlite"), EventBudget::default()).unwrap();
    let cards: Vec<String> = (0..count)
        .map(|number| format!("summary-card-{number}"))
        .collect();
    for card in &cards {
        store
            .create_local(
                &format!("seed-{card}"),
                &CardRecord::new(card, "org.example.summary", 1, "Initial", vec![]).unwrap(),
            )
            .unwrap();
    }
    let mut host = HostRuntime::new(store).unwrap();
    let mut connection = host.connect().unwrap();
    for card in &cards {
        // Preparation and all subsequent card grants share the same monotonic
        // host time; a later card must not reset time behind an earlier edit.
        host.grant(&mut connection, GrantKind::EditContent, card, EXPIRES, 1)
            .unwrap();
        host.edit_content(
            &connection,
            &ContentChange {
                operation_id: format!("prepare-{card}"),
                card_id: card.clone(),
                expected_revision: 1,
                title: "private-query-summary-marker".into(),
                body: Vec::new(),
                preview_text: preview.into(),
                attachments: None,
            },
            || 1,
        )
        .unwrap();
        host.grant(&mut connection, GrantKind::ReadSummary, card, EXPIRES, 1)
            .unwrap();
    }
    let content = ContentHost::new(&host, &connection, cards.clone()).unwrap();
    (temp, host, connection, content, cards)
}

#[test]
fn sixteen_individually_legal_large_summaries_return_a_bounded_outer_limit_refusal() {
    let preview = "large-private-summary-marker:".to_owned() + &"x".repeat(16 * 1024 - 29);
    assert_eq!(preview.len(), 16 * 1024);
    let (_temp, mut host, connection, mut content, cards) = summary_fixture(16, &preview);
    let mut individually_encoded_bytes = 0;
    for card in &cards {
        let record = host.store_local().card(card).unwrap().unwrap();
        assert_eq!(record.summary().preview_text.len(), 16 * 1024);
        let command = Command::ReadSummary {
            request_id: "individual-summary".into(),
            card_id: card.clone(),
        };
        let wire = host
            .dispatch(&connection, &command.encode().unwrap(), || 2)
            .unwrap();
        assert!(matches!(
            Response::decode(&wire).unwrap().outcome,
            CoreOutcome::Summary(_)
        ));
        individually_encoded_bytes += wire.len();
    }
    assert!(individually_encoded_bytes > morrow_agent_content_v1::MAX_FRAME_BYTES);
    let request = Request::new("large-query", Action::Query { cards }).unwrap();
    let wire = content
        .dispatch(&mut host, &connection, request.wire(), || 2)
        .unwrap();
    assert!(wire.len() <= morrow_agent_content_v1::MAX_FRAME_BYTES);
    assert!(matches!(
        Reply::decode_for(&wire, &request).unwrap().outcome,
        Outcome::Rejected {
            failure: Failure::Limit
        }
    ));
    assert!(!contains_plaintext(&wire, b"large-private-summary-marker:"));
}

#[test]
fn final_outer_query_expiry_withdraws_every_previously_successful_projection() {
    let marker = b"private-query-summary-marker";
    let (_temp, mut host, connection, mut content, cards) =
        summary_fixture(2, "query body preview");
    let request = Request::new("calibrate-query-clock", Action::Query { cards }).unwrap();
    let mut successful_samples = 0;
    let wire = content
        .dispatch(&mut host, &connection, request.wire(), || {
            successful_samples += 1;
            2
        })
        .unwrap();
    let Outcome::Query { responses } = Reply::decode_for(&wire, &request).unwrap().outcome else {
        panic!("ordinary query must succeed")
    };
    assert_eq!(responses.len(), 2);
    assert!(responses.iter().all(|response| matches!(
        Response::decode(response).unwrap().outcome,
        CoreOutcome::Summary(_)
    )));
    assert!(contains_plaintext(&wire, marker));
    assert!(successful_samples > 1);

    // Calibrate the actual last clock observation from a successful execution,
    // then use a fresh original Store/runtime so time never moves backwards.
    let (_late_temp, mut late_host, late_connection, mut late_content, late_cards) =
        summary_fixture(2, "query body preview");
    let request = Request::new("late-query-clock", Action::Query { cards: late_cards }).unwrap();
    let mut samples = 0;
    let wire = late_content
        .dispatch(&mut late_host, &late_connection, request.wire(), || {
            samples += 1;
            if samples < successful_samples {
                2
            } else {
                EXPIRES
            }
        })
        .unwrap();
    assert_eq!(samples, successful_samples);
    assert!(matches!(
        Reply::decode_for(&wire, &request).unwrap().outcome,
        Outcome::Rejected {
            failure: Failure::Denied
        }
    ));
    assert!(!contains_plaintext(&wire, marker));
    assert!(!contains_plaintext(&wire, b"query body preview"));
}

#[test]
fn final_outer_read_expiry_withdraws_the_encoded_content_bytes() {
    let mut fixture = Fixture::new();
    let request = Request::new(
        "calibrate-read-clock",
        Action::ReadRef {
            reference: fixture.reference(),
            offset: 0,
            length: ORIGINAL.len() as u32,
        },
    )
    .unwrap();
    let mut successful_samples = 0;
    let wire = fixture
        .content
        .dispatch(
            &mut fixture.host,
            &fixture.connection,
            request.wire(),
            || {
                successful_samples += 1;
                2
            },
        )
        .unwrap();
    assert!(matches!(
        Reply::decode_for(&wire, &request).unwrap().outcome,
        Outcome::ReadRef { .. }
    ));
    assert!(contains_plaintext(&wire, ORIGINAL));
    assert!(successful_samples > 1);

    let mut late = Fixture::new();
    let request = Request::new(
        "late-read-clock",
        Action::ReadRef {
            reference: late.reference(),
            offset: 0,
            length: ORIGINAL.len() as u32,
        },
    )
    .unwrap();
    let mut samples = 0;
    let wire = late
        .content
        .dispatch(&mut late.host, &late.connection, request.wire(), || {
            samples += 1;
            if samples < successful_samples {
                2
            } else {
                EXPIRES
            }
        })
        .unwrap();
    assert_eq!(samples, successful_samples);
    assert!(matches!(
        Reply::decode_for(&wire, &request).unwrap().outcome,
        Outcome::Rejected {
            failure: Failure::Denied
        }
    ));
    assert!(!contains_plaintext(&wire, ORIGINAL));
    assert_eq!(late.body(), ORIGINAL);
    assert_eq!(late.revision(), 1);
}

#[test]
fn a_real_read_summary_only_package_cannot_gain_read_or_edit_from_nominations() {
    use morrow_core::plugin_package::{Package, proto::Capability};
    let mut fixture = Fixture::new();
    let module = b"\0asm\x01\0\0\0";
    let package = Package::build(
        Package::manifest_for(
            "org.example.summary-only",
            "1.0.0",
            module,
            vec![Capability::ReadSummary],
        ),
        module,
    )
    .unwrap();
    let mut connection = fixture.host.connect_package(&package).unwrap();
    assert_eq!(connection.package_digest(), Some(package.digest()));
    assert!(
        fixture
            .host
            .grant(&mut connection, GrantKind::ReadContent, CARD, EXPIRES, 0)
            .is_err()
    );
    assert!(
        fixture
            .host
            .grant(&mut connection, GrantKind::EditContent, CARD, EXPIRES, 0)
            .is_err()
    );
    fixture
        .host
        .grant(&mut connection, GrantKind::ReadSummary, CARD, EXPIRES, 0)
        .unwrap();
    let mut content = ContentHost::new(&fixture.host, &connection, vec![CARD.into()]).unwrap();
    let request = Request::new(
        "package-summary",
        Action::Query {
            cards: vec![CARD.into()],
        },
    )
    .unwrap();
    let wire = content
        .dispatch(&mut fixture.host, &connection, request.wire(), || 1)
        .unwrap();
    let Outcome::Query { responses } = Reply::decode_for(&wire, &request).unwrap().outcome else {
        panic!("declared summary read must succeed")
    };
    assert!(matches!(
        Response::decode(&responses[0]).unwrap().outcome,
        CoreOutcome::Summary(_)
    ));
    let request = Request::new(
        "package-body",
        Action::ReadRef {
            reference: fixture.reference(),
            offset: 0,
            length: 1,
        },
    )
    .unwrap();
    let wire = content
        .dispatch(&mut fixture.host, &connection, request.wire(), || 1)
        .unwrap();
    rejected(Reply::decode_for(&wire, &request).unwrap().outcome);
    let proposal = proposal_request("package-edit", fixture.reference(), EDITED);
    let wire = content
        .dispatch(&mut fixture.host, &connection, proposal.wire(), || 1)
        .unwrap();
    rejected(Reply::decode_for(&wire, &proposal).unwrap().outcome);
    assert!(
        content
            .approve(
                &fixture.host,
                &connection,
                "package-edit",
                proposal.digest(),
                1
            )
            .is_err()
    );
    assert_eq!(content.phase("package-edit"), None);
    assert_eq!(fixture.body(), ORIGINAL);
    assert_eq!(fixture.revision(), 1);
}

#[test]
fn nominated_query_and_exact_read_use_the_original_runtime_grants() {
    let mut fixture = Fixture::new();
    let query = Request::new(
        "query-card",
        Action::Query {
            cards: vec![CARD.into()],
        },
    )
    .unwrap();
    let Outcome::Query { responses } = fixture.dispatch(&query, 1) else {
        panic!("query must succeed")
    };
    assert_eq!(responses.len(), 1);
    let CoreOutcome::Summary(summary) = Response::decode(&responses[0]).unwrap().outcome else {
        panic!("original summary required")
    };
    assert_eq!(summary.id, CARD);
    assert_eq!(summary.revision, 1);
    let read = Request::new(
        "read-card",
        Action::ReadRef {
            reference: fixture.reference(),
            offset: 1,
            length: 5,
        },
    )
    .unwrap();
    let Outcome::ReadRef { response } = fixture.dispatch(&read, 1) else {
        panic!("read must succeed")
    };
    let CoreOutcome::ContentChunk(part) = Response::decode(&response).unwrap().outcome else {
        panic!("original content chunk required")
    };
    assert_eq!(part.card_id, CARD);
    assert_eq!(part.revision, 1);
    assert_eq!(part.offset, 1);
    assert_eq!(part.bytes, ORIGINAL[1..6]);
    assert_eq!(part.total_length, ORIGINAL.len() as u64);
    let expected_digest: [u8; 32] = Sha256::digest(ORIGINAL).into();
    assert_eq!(part.body_sha256, expected_digest);
}

#[test]
fn a_granted_sibling_is_not_added_to_the_host_nominated_scope() {
    let mut fixture = Fixture::new();
    let request = Request::new(
        "query-sibling",
        Action::Query {
            cards: vec![CARD.into(), SIBLING.into()],
        },
    )
    .unwrap();
    rejected(fixture.dispatch(&request, 1));
    let mut reference = fixture.reference();
    reference.card_id = SIBLING.into();
    let request = Request::new(
        "read-sibling",
        Action::ReadRef {
            reference,
            offset: 0,
            length: 1,
        },
    )
    .unwrap();
    rejected(fixture.dispatch(&request, 1));
    assert_eq!(fixture.body(), ORIGINAL);
}

#[test]
fn reference_length_digest_and_revision_must_match_the_exact_current_object() {
    for mutation in 0..3 {
        let mut fixture = Fixture::new();
        let mut reference = fixture.reference();
        match mutation {
            0 => reference.total_length += 1,
            1 => reference.body_sha256[0] ^= 1,
            _ => reference.revision += 1,
        }
        let read = Request::new(
            "wrong-reference",
            Action::ReadRef {
                reference: reference.clone(),
                offset: 0,
                length: 1,
            },
        )
        .unwrap();
        rejected(fixture.dispatch(&read, 1));
        let proposal = proposal_request("wrong-ref-edit", reference, EDITED);
        rejected(fixture.dispatch(&proposal, 1));
        assert_eq!(fixture.body(), ORIGINAL);
        assert_eq!(fixture.revision(), 1);
    }
}

#[test]
fn propose_and_approve_have_no_effect_then_one_original_cas_commit_survives_reopen() {
    let mut fixture = Fixture::new();
    let proposal = fixture.propose("one-edit");
    assert_eq!(
        fixture.content.phase("one-edit"),
        Some(ProposalPhase::Proposed)
    );
    assert_eq!(fixture.body(), ORIGINAL);
    assert_eq!(fixture.revision(), 1);
    assert!(matches!(
        fixture
            .host
            .store_local()
            .lookup_for_card(CARD, "one-edit")
            .unwrap(),
        Lookup::Absent
    ));
    fixture.approve(&proposal, "one-edit", 2);
    assert_eq!(
        fixture.content.phase("one-edit"),
        Some(ProposalPhase::Approved)
    );
    assert_eq!(fixture.body(), ORIGINAL);
    assert_eq!(fixture.revision(), 1);
    let committed = fixture
        .content
        .execute_approved(&mut fixture.host, &fixture.connection, "one-edit", || 3)
        .unwrap();
    let CoreOutcome::ContentCommitted(receipt) = committed.outcome else {
        panic!("exact core receipt required")
    };
    assert_eq!(receipt.operation_id, "one-edit");
    assert_eq!(receipt.card_id, CARD);
    assert_eq!(receipt.revision, 2);
    assert_eq!(
        fixture.content.phase("one-edit"),
        Some(ProposalPhase::LocallyCommitted)
    );
    assert!(
        fixture
            .content
            .execute_approved(&mut fixture.host, &fixture.connection, "one-edit", || 4)
            .is_err()
    );
    assert_eq!(fixture.body(), EDITED);
    assert_eq!(fixture.revision(), 2);
    drop(fixture.content);
    drop(fixture.connection);
    drop(fixture.host);
    let saved = Store::open_existing(&fixture.path, EventBudget::default()).unwrap();
    saved.integrity_check().unwrap();
    assert_eq!(saved.card(CARD).unwrap().unwrap().body(), EDITED);
    assert!(matches!(
        saved.lookup_for_card(CARD, "one-edit").unwrap(),
        Lookup::Committed(_)
    ));
}

#[test]
fn trusted_approval_binds_the_original_full_proposal_digest() {
    let mut fixture = Fixture::new();
    let proposal = fixture.propose("approval-hash");
    let mut wrong = proposal.digest();
    wrong[0] ^= 1;
    assert!(
        fixture
            .content
            .approve(
                &fixture.host,
                &fixture.connection,
                "approval-hash",
                wrong,
                2
            )
            .is_err()
    );
    assert_eq!(
        fixture.content.phase("approval-hash"),
        Some(ProposalPhase::Proposed)
    );
    assert!(
        fixture
            .content
            .execute_approved(
                &mut fixture.host,
                &fixture.connection,
                "approval-hash",
                || 3
            )
            .is_err()
    );
    assert_eq!(fixture.body(), ORIGINAL);
    fixture.approve(&proposal, "approval-hash", 3);
    assert_eq!(
        fixture.content.phase("approval-hash"),
        Some(ProposalPhase::Approved)
    );
}

#[test]
fn a_proposal_retry_requires_the_original_complete_frame_and_preserves_the_first_proposal() {
    let mut fixture = Fixture::new();
    let proposal = fixture.propose("repeat-proposal");
    let Outcome::Proposed {
        proposal_sha256, ..
    } = fixture.dispatch(&proposal, 1)
    else {
        panic!("exact retry must retain proposal")
    };
    assert_eq!(proposal_sha256, proposal.digest());
    let different = Request::new("another-request-id", proposal.action().clone()).unwrap();
    rejected(fixture.dispatch(&different, 1));
    fixture.approve(&proposal, "repeat-proposal", 2);
    let response = fixture
        .content
        .execute_approved(
            &mut fixture.host,
            &fixture.connection,
            "repeat-proposal",
            || 3,
        )
        .unwrap();
    assert!(matches!(response.outcome, CoreOutcome::ContentCommitted(_)));
    assert_eq!(fixture.revision(), 2);
}

#[test]
fn the_original_host_and_connection_identity_bind_read_approval_and_execution() {
    let mut fixture = Fixture::new();
    let read = Request::new(
        "identity-read",
        Action::ReadRef {
            reference: fixture.reference(),
            offset: 0,
            length: 1,
        },
    )
    .unwrap();
    let proposal = fixture.propose("identity-edit");
    let other = Fixture::new();
    assert!(
        fixture
            .content
            .approve(
                &other.host,
                &other.connection,
                "identity-edit",
                proposal.digest(),
                2
            )
            .is_err()
    );
    let another_connection = fixture.host.connect().unwrap();
    assert!(
        fixture
            .content
            .approve(
                &fixture.host,
                &another_connection,
                "identity-edit",
                proposal.digest(),
                2
            )
            .is_err()
    );
    let raw = fixture
        .content
        .dispatch(
            &mut fixture.host,
            &another_connection,
            &read.encode().unwrap(),
            || 2,
        )
        .unwrap();
    rejected(Reply::decode_for(&raw, &read).unwrap().outcome);
    fixture.approve(&proposal, "identity-edit", 2);
    assert!(
        fixture
            .content
            .execute_approved(
                &mut fixture.host,
                &another_connection,
                "identity-edit",
                || 3
            )
            .is_err()
    );
    assert_eq!(fixture.body(), ORIGINAL);
    let response = fixture
        .content
        .execute_approved(
            &mut fixture.host,
            &fixture.connection,
            "identity-edit",
            || 3,
        )
        .unwrap();
    assert!(matches!(response.outcome, CoreOutcome::ContentCommitted(_)));
}

#[test]
fn revoked_or_expired_original_read_grants_cannot_deliver_content_or_issue_proposals() {
    for expire in [false, true] {
        let mut fixture = Fixture::new();
        let read = Request::new(
            "read-after-authority-loss",
            Action::ReadRef {
                reference: fixture.reference(),
                offset: 0,
                length: 1,
            },
        )
        .unwrap();
        let proposal = proposal_request("authority-loss", fixture.reference(), EDITED);
        let now = if expire {
            EXPIRES
        } else {
            fixture
                .host
                .revoke(&mut fixture.connection, GrantKind::ReadContent, CARD)
                .unwrap();
            2
        };
        rejected(fixture.dispatch(&read, now));
        rejected(fixture.dispatch(&proposal, now));
        assert_eq!(fixture.body(), ORIGINAL);
    }
}

#[test]
fn approval_does_not_replace_revoked_edit_or_read_authorization() {
    for kind in [GrantKind::EditContent, GrantKind::ReadContent] {
        let mut fixture = Fixture::new();
        let proposal = fixture.propose("revoked-approved");
        fixture.approve(&proposal, "revoked-approved", 2);
        fixture
            .host
            .revoke(&mut fixture.connection, kind, CARD)
            .unwrap();
        match fixture.content.execute_approved(
            &mut fixture.host,
            &fixture.connection,
            "revoked-approved",
            || 3,
        ) {
            Ok(response) => assert!(matches!(response.outcome, CoreOutcome::Rejected(_))),
            Err(_) => {}
        }
        assert_eq!(fixture.body(), ORIGINAL);
        assert_eq!(fixture.revision(), 1);
        assert!(
            fixture
                .content
                .execute_approved(
                    &mut fixture.host,
                    &fixture.connection,
                    "revoked-approved",
                    || 4
                )
                .is_err()
        );
    }
}

#[test]
fn approved_proposals_cannot_overwrite_a_newer_card_revision() {
    let mut fixture = Fixture::new();
    let proposal = fixture.propose("stale-approved");
    fixture.approve(&proposal, "stale-approved", 2);
    let competing = Command::EditContent(ContentChange {
        operation_id: "competing-edit".into(),
        card_id: CARD.into(),
        expected_revision: 1,
        title: "Concurrent".into(),
        body: b"other author".to_vec(),
        preview_text: String::new(),
        attachments: None,
    });
    let response = fixture
        .host
        .dispatch(&fixture.connection, &competing.encode().unwrap(), || 3)
        .unwrap();
    assert!(matches!(
        Response::decode(&response).unwrap().outcome,
        CoreOutcome::ContentCommitted(_)
    ));
    match fixture.content.execute_approved(
        &mut fixture.host,
        &fixture.connection,
        "stale-approved",
        || 4,
    ) {
        Ok(response) => assert!(matches!(response.outcome, CoreOutcome::Rejected(_))),
        Err(_) => {}
    }
    assert_eq!(fixture.body(), b"other author");
    assert_eq!(fixture.revision(), 2);
    assert!(matches!(
        fixture
            .host
            .store_local()
            .lookup_for_card(CARD, "stale-approved")
            .unwrap(),
        Lookup::Absent
    ));
    assert!(
        fixture
            .content
            .execute_approved(
                &mut fixture.host,
                &fixture.connection,
                "stale-approved",
                || 5
            )
            .is_err()
    );
}

#[test]
fn operation_history_is_read_only_and_never_restores_a_missing_live_grant() {
    let mut fixture = Fixture::new();
    let proposal = fixture.propose("history-edit");
    fixture.approve(&proposal, "history-edit", 2);
    fixture
        .content
        .execute_approved(&mut fixture.host, &fixture.connection, "history-edit", || 3)
        .unwrap();
    fixture
        .host
        .revoke(&mut fixture.connection, GrantKind::EditContent, CARD)
        .unwrap();
    let inspect = Request::new(
        "history-query",
        Action::InspectOperation {
            card_id: CARD.into(),
            operation_id: "history-edit".into(),
        },
    )
    .unwrap();
    let Outcome::Operation { response } = fixture.dispatch(&inspect, 4) else {
        panic!("history is separately read-authorized")
    };
    assert!(matches!(
        Response::decode(&response).unwrap().outcome,
        CoreOutcome::OperationResult {
            result: Lookup::Committed(_),
            ..
        }
    ));
    assert_eq!(
        fixture.content.phase("history-edit"),
        Some(ProposalPhase::LocallyCommitted)
    );
    assert!(
        fixture
            .content
            .execute_approved(&mut fixture.host, &fixture.connection, "history-edit", || 4)
            .is_err()
    );
    fixture
        .host
        .revoke(&mut fixture.connection, GrantKind::QueryOperation, CARD)
        .unwrap();
    rejected(fixture.dispatch(&inspect, 4));
    assert_eq!(fixture.body(), EDITED);
    assert_eq!(fixture.revision(), 2);
}

#[test]
fn late_expiry_after_real_commit_stays_unknown_until_read_only_history_reconciliation() {
    let mut observed_late_commit = false;
    // Use fresh stores to find the original post-commit delivery boundary without
    // changing the production clock implementation or re-executing any operation.
    for before_expiry in 1..=32 {
        let mut fixture = Fixture::new();
        let proposal = fixture.propose("late-commit");
        fixture.approve(&proposal, "late-commit", 2);
        let mut samples = 0;
        let result = fixture.content.execute_approved(
            &mut fixture.host,
            &fixture.connection,
            "late-commit",
            || {
                samples += 1;
                if samples <= before_expiry { 3 } else { EXPIRES }
            },
        );
        let persisted = matches!(
            fixture
                .host
                .store_local()
                .lookup_for_card(CARD, "late-commit")
                .unwrap(),
            Lookup::Committed(_)
        );
        let exact_receipt_delivered = result
            .as_ref()
            .is_ok_and(|response| matches!(response.outcome, CoreOutcome::ContentCommitted(_)));
        if persisted && !exact_receipt_delivered {
            observed_late_commit = true;
            assert_eq!(
                fixture.content.phase("late-commit"),
                Some(ProposalPhase::DispatchUnknown)
            );
            assert_eq!(fixture.body(), EDITED);
            assert_eq!(fixture.revision(), 2);
            assert!(
                fixture
                    .content
                    .execute_approved(
                        &mut fixture.host,
                        &fixture.connection,
                        "late-commit",
                        || EXPIRES
                    )
                    .is_err()
            );
            // History needs a newly issued QueryOperation grant; it is not replay.
            fixture
                .host
                .grant(
                    &mut fixture.connection,
                    GrantKind::QueryOperation,
                    CARD,
                    EXPIRES + 20,
                    EXPIRES,
                )
                .unwrap();
            let inspect = Request::new(
                "inspect-late",
                Action::InspectOperation {
                    card_id: CARD.into(),
                    operation_id: "late-commit".into(),
                },
            )
            .unwrap();
            let Outcome::Operation { response } = fixture.dispatch(&inspect, EXPIRES) else {
                panic!("new read-only history grant required")
            };
            assert!(matches!(
                Response::decode(&response).unwrap().outcome,
                CoreOutcome::OperationResult {
                    result: Lookup::Committed(_),
                    ..
                }
            ));
            assert_eq!(
                fixture.content.phase("late-commit"),
                Some(ProposalPhase::DispatchUnknown)
            );
            assert!(
                fixture
                    .content
                    .execute_approved(
                        &mut fixture.host,
                        &fixture.connection,
                        "late-commit",
                        || EXPIRES
                    )
                    .is_err()
            );
            assert_eq!(fixture.revision(), 2);
            break;
        }
    }
    assert!(
        observed_late_commit,
        "must actually observe persisted commit with withheld exact receipt"
    );
}

#[test]
fn proposals_are_bounded_and_never_create_content_effects() {
    let mut fixture = Fixture::new();
    for number in 0..32 {
        fixture.propose(&format!("bounded-{number}"));
    }
    let excess = proposal_request("bounded-excess", fixture.reference(), EDITED);
    rejected(fixture.dispatch(&excess, 1));
    assert_eq!(fixture.body(), ORIGINAL);
    assert_eq!(fixture.revision(), 1);
    assert!(matches!(
        fixture
            .host
            .store_local()
            .lookup_for_card(CARD, "bounded-excess")
            .unwrap(),
        Lookup::Absent
    ));
}
