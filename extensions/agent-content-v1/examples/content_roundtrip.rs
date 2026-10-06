//! A disposable ordinary SQLite Store and the original host authorization path.
//! No native process transport, production database or external content sending.
use morrow_agent_content_v1::{Action, ContentRef, Outcome, Reply, Request, host::ContentHost};
use morrow_core::{
    content::CardRecord,
    content_change::ContentChange,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    response::{Outcome as CoreOutcome, Response},
    runtime::Command,
    store::{EventBudget, Store},
    transaction::Lookup,
};
use sha2::{Digest, Sha256};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let mut store = Store::open(
        &temporary.path().join("content.sqlite"),
        EventBudget::default(),
    )?;
    let body = b"synthetic original body";
    store.create_local(
        "seed",
        &CardRecord::new("card", "note", 1, "Original", body.to_vec())?,
    )?;
    let mut runtime = HostRuntime::new(store)?;
    let mut connection = runtime.connect()?;
    for kind in [
        GrantKind::ReadSummary,
        GrantKind::ReadContent,
        GrantKind::EditContent,
        GrantKind::QueryOperation,
    ] {
        runtime.grant(&mut connection, kind, "card", 100, 0)?;
    }
    let mut host = ContentHost::new(&runtime, &connection, vec!["card".into()])?;
    let reference = ContentRef {
        card_id: "card".into(),
        revision: 1,
        total_length: body.len() as u64,
        body_sha256: Sha256::digest(body).into(),
    };
    let requests = [
        Request::new(
            "query",
            Action::Query {
                cards: vec!["card".into()],
            },
        )?,
        Request::new(
            "read",
            Action::ReadRef {
                reference: reference.clone(),
                offset: 0,
                length: 32768,
            },
        )?,
    ];
    for request in requests {
        let bytes = host.dispatch(&mut runtime, &connection, request.wire(), || 1)?;
        let reply = Reply::decode_for(&bytes, &request)?;
        match reply.outcome {
            Outcome::Query { responses } => assert!(matches!(
                Response::decode(&responses[0])?.outcome,
                CoreOutcome::Summary(_)
            )),
            Outcome::ReadRef { response } => {
                let CoreOutcome::ContentChunk(part) = Response::decode(&response)?.outcome else {
                    panic!("expected content")
                };
                assert_eq!(part.bytes, body);
            }
            _ => panic!("expected authorized content response"),
        }
    }
    let command = Command::EditContent(ContentChange {
        operation_id: "edit".into(),
        card_id: "card".into(),
        expected_revision: 1,
        title: "Edited".into(),
        body: b"synthetic approved body".to_vec(),
        preview_text: String::new(),
        attachments: None,
    });
    let proposal = Request::new(
        "proposal",
        Action::ProposeMutation {
            reference,
            command: command.encode()?,
        },
    )?;
    let bytes = host.dispatch(&mut runtime, &connection, proposal.wire(), || 2)?;
    let Outcome::Proposed {
        operation_id,
        proposal_sha256,
    } = Reply::decode_for(&bytes, &proposal)?.outcome
    else {
        panic!("expected proposal")
    };
    assert_eq!(
        runtime
            .store_local()
            .card("card")?
            .unwrap()
            .summary()
            .revision,
        1
    );
    host.approve(&runtime, &connection, &operation_id, proposal_sha256, 3)?;
    let CoreOutcome::ContentCommitted(receipt) = host
        .execute_approved(&mut runtime, &connection, &operation_id, || 4)?
        .outcome
    else {
        panic!("expected exact receipt")
    };
    assert_eq!(receipt.revision, 2);
    assert!(
        host.execute_approved(&mut runtime, &connection, &operation_id, || 4)
            .is_err()
    );
    let inspect = Request::new(
        "inspect",
        Action::InspectOperation {
            card_id: "card".into(),
            operation_id,
        },
    )?;
    let bytes = host.dispatch(&mut runtime, &connection, inspect.wire(), || 5)?;
    let Outcome::Operation { response } = Reply::decode_for(&bytes, &inspect)?.outcome else {
        panic!("expected history")
    };
    assert!(matches!(
        Response::decode(&response)?.outcome,
        CoreOutcome::OperationResult {
            result: Lookup::Committed(_),
            ..
        }
    ));
    println!(
        "PASS: Query, ReadRef, proposal without write, trusted approval, one CAS commit, history inspection; revision=2."
    );
    Ok(())
}
