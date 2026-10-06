//! Complete-body verification against hostile callbacks and a real ordinary
//! HostRuntime/Store chain. No protected/native transport qualification.
use morrow_agent_content_v1::{
    Action, ContentRef, Error, MAX_CONTENT_BYTES, MAX_READ_BYTES, Outcome, Reply, Request,
    client::read_complete,
};
use morrow_core::{
    response::{Failure, Outcome as CoreOutcome, Response},
    runtime::ContentChunk,
};
use sha2::{Digest, Sha256};

fn body(length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| (index.wrapping_mul(71) % 251) as u8)
        .collect()
}
fn reference(body: &[u8]) -> ContentRef {
    ContentRef {
        card_id: "content-card".into(),
        revision: 1,
        total_length: body.len() as u64,
        body_sha256: Sha256::digest(body).into(),
    }
}
fn response(request: &Request, body: &[u8], edit: impl FnOnce(&mut ContentChunk)) -> Vec<u8> {
    let Action::ReadRef {
        reference,
        offset,
        length,
    } = request.action()
    else {
        panic!("read only");
    };
    let end = (*offset as usize + *length as usize).min(body.len());
    let mut chunk = ContentChunk {
        card_id: reference.card_id.clone(),
        revision: reference.revision,
        offset: *offset,
        total_length: reference.total_length,
        body_sha256: reference.body_sha256,
        bytes: body[*offset as usize..end].to_vec(),
    };
    edit(&mut chunk);
    let nested = Response {
        request_id: request.request_id().into(),
        outcome: CoreOutcome::ContentChunk(chunk),
    }
    .encode()
    .unwrap();
    // Skip constructor correlation checks to model an untrusted peer.
    Reply {
        request_id: request.request_id().into(),
        request_sha256: request.digest(),
        outcome: Outcome::ReadRef { response: nested },
    }
    .encode()
    .unwrap()
}
fn denied(request: &Request) -> Vec<u8> {
    Reply::new(
        request,
        Outcome::Rejected {
            failure: Failure::Denied,
        },
    )
    .unwrap()
    .encode()
    .unwrap()
}

#[test]
fn seventy_kib_uses_ordered_bounded_chunks_and_returns_complete_verified_bytes() {
    let source = body(70 * 1024);
    let reference = reference(&source);
    let mut calls = Vec::new();
    let result = read_complete(&reference, "whole-read", source.len(), |request| {
        let Action::ReadRef { offset, length, .. } = request.action() else {
            panic!();
        };
        calls.push((request.request_id().to_owned(), *offset, *length));
        Ok(response(request, &source, |_| {}))
    })
    .unwrap();
    assert_eq!(
        calls,
        vec![
            ("whole-read-1".into(), 0, MAX_READ_BYTES),
            ("whole-read-2".into(), MAX_READ_BYTES as u64, MAX_READ_BYTES),
            ("whole-read-3".into(), 2 * MAX_READ_BYTES as u64, 6 * 1024),
        ]
    );
    assert_eq!(result.reference(), &reference);
    assert_eq!(result.body(), source);
    assert_eq!(result.into_bytes(), source);
}

#[test]
fn empty_content_still_requires_one_real_authorization_response() {
    let reference = reference(&[]);
    let mut calls = 0;
    let result = read_complete(&reference, "empty", 0, |request| {
        calls += 1;
        assert_eq!(request.request_id(), "empty-1");
        let Action::ReadRef { offset, length, .. } = request.action() else {
            panic!();
        };
        assert_eq!((*offset, *length), (0, 1));
        Ok(response(request, &[], |_| {}))
    })
    .unwrap();
    assert_eq!(calls, 1);
    assert!(result.body().is_empty());
    let mut denied_calls = 0;
    assert_eq!(
        read_complete(&reference, "empty-denied", 0, |request| {
            denied_calls += 1;
            Ok(denied(request))
        }),
        Err(Error::Invalid)
    );
    assert_eq!(denied_calls, 1);
}

#[test]
fn denial_after_partial_read_stops_without_retry_or_verified_value() {
    let source = body(70 * 1024);
    let mut calls = 0;
    let result = read_complete(&reference(&source), "partial", source.len(), |request| {
        calls += 1;
        if calls == 2 {
            Ok(denied(request))
        } else {
            Ok(response(request, &source, |_| {}))
        }
    });
    assert_eq!(result, Err(Error::Invalid));
    assert_eq!(calls, 2);
}

#[test]
fn nested_core_denial_after_partial_read_stops_without_retry() {
    let source = body(70 * 1024);
    let mut calls = 0;
    let result = read_complete(
        &reference(&source),
        "nested-denial",
        source.len(),
        |request| {
            calls += 1;
            if calls != 2 {
                return Ok(response(request, &source, |_| {}));
            }
            let nested = Response {
                request_id: request.request_id().into(),
                outcome: CoreOutcome::Rejected(Failure::Denied),
            }
            .encode()
            .unwrap();
            Reply::new(request, Outcome::ReadRef { response: nested })?.encode()
        },
    );
    assert_eq!(result, Err(Error::Invalid));
    assert_eq!(calls, 2);
}

#[test]
fn callback_error_after_partial_read_is_preserved_and_never_retried() {
    let source = body(70 * 1024);
    let mut calls = 0;
    let result = read_complete(&reference(&source), "transport", source.len(), |request| {
        calls += 1;
        if calls == 2 {
            Err(Error::Contract)
        } else {
            Ok(response(request, &source, |_| {}))
        }
    });
    assert_eq!(result, Err(Error::Contract));
    assert_eq!(calls, 2);
}

#[test]
fn explicit_caller_budget_and_invalid_reference_refuse_before_exchange() {
    let source = body(64);
    let mut calls = 0;
    let mut no_exchange = |_: &Request| {
        calls += 1;
        panic!("budget/ref must reject first")
    };
    assert_eq!(
        read_complete(&reference(&source), "budget", 63, &mut no_exchange),
        Err(Error::Limit)
    );
    let mut oversized = reference(&source);
    oversized.total_length = MAX_CONTENT_BYTES + 1;
    assert_eq!(
        read_complete(&oversized, "global", usize::MAX, &mut no_exchange),
        Err(Error::Limit)
    );
    let mut zero_revision = reference(&source);
    zero_revision.revision = 0;
    assert_eq!(
        read_complete(&zero_revision, "revision", 64, &mut no_exchange),
        Err(Error::Limit)
    );
    assert_eq!(calls, 0);
}

#[test]
fn malformed_or_future_overlength_request_ids_refuse_before_any_exchange() {
    let source = body(10 * MAX_READ_BYTES as usize);
    let mut calls = 0;
    for prefix in ["bad/path".to_owned(), "bad\n".to_owned(), "x".repeat(254)] {
        assert_eq!(
            read_complete(&reference(&source), &prefix, source.len(), |_| {
                calls += 1;
                panic!("all suffixes must validate before exchange")
            }),
            Err(Error::Invalid)
        );
    }
    assert_eq!(calls, 0);
}

#[test]
fn metadata_mismatch_is_refused_at_first_chunk() {
    let source = body(70 * 1024);
    for field in 0..5 {
        let mut calls = 0;
        let result = read_complete(&reference(&source), "metadata", source.len(), |request| {
            calls += 1;
            Ok(response(request, &source, |chunk| match field {
                0 => chunk.card_id = "other-card".into(),
                1 => chunk.revision += 1,
                2 => chunk.total_length += 1,
                3 => chunk.body_sha256 = [0; 32],
                4 => chunk.bytes.pop().map(|_| ()).unwrap(),
                _ => unreachable!(),
            }))
        });
        assert_eq!(result, Err(Error::Correlation));
        assert_eq!(calls, 1);
    }
}

#[test]
fn tampered_chunk_with_consistent_reference_metadata_fails_whole_body_sha() {
    let source = body(70 * 1024);
    let mut calls = 0;
    let result = read_complete(&reference(&source), "whole-hash", source.len(), |request| {
        calls += 1;
        Ok(response(request, &source, |chunk| {
            if chunk.offset == 0 {
                chunk.bytes[1] ^= 1;
            }
        }))
    });
    assert_eq!(result, Err(Error::Correlation));
    assert_eq!(calls, 3); // No partial success even when every chunk metadata agrees.
}

#[test]
fn empty_body_wrong_digest_cannot_be_declared_verified() {
    let mut reference = reference(&[]);
    reference.body_sha256 = [0; 32];
    let mut calls = 0;
    assert_eq!(
        read_complete(&reference, "empty-hash", 0, |request| {
            calls += 1;
            Ok(response(request, &[], |_| {}))
        }),
        Err(Error::Correlation)
    );
    assert_eq!(calls, 1);
}

#[test]
fn repeated_offset_in_later_chunk_is_refused_and_no_further_read_occurs() {
    let source = body(70 * 1024);
    let mut calls = 0;
    let result = read_complete(&reference(&source), "repeat", source.len(), |request| {
        calls += 1;
        Ok(response(request, &source, |chunk| {
            if calls == 2 {
                chunk.offset = 0;
            }
        }))
    });
    assert_eq!(result, Err(Error::Correlation));
    assert_eq!(calls, 2);
}

#[test]
fn wrong_outcome_and_outer_request_identity_are_refused_without_retry() {
    let source = body(70 * 1024);
    for wrong in 0..3 {
        let mut calls = 0;
        let result = read_complete(
            &reference(&source),
            "wrong-reply",
            source.len(),
            |request| {
                calls += 1;
                if wrong == 0 {
                    return Reply {
                        request_id: request.request_id().into(),
                        request_sha256: request.digest(),
                        outcome: Outcome::Proposed {
                            operation_id: "unexpected-proposal".into(),
                            proposal_sha256: request.digest(),
                        },
                    }
                    .encode();
                }
                let mut reply = Reply::new(
                    request,
                    Outcome::Rejected {
                        failure: Failure::Denied,
                    },
                )
                .unwrap();
                if wrong == 1 {
                    reply.request_id = "other-request".into();
                } else {
                    reply.request_sha256 = [0; 32];
                }
                reply.encode()
            },
        );
        assert_eq!(result, Err(Error::Correlation));
        assert_eq!(calls, 1);
    }
}

#[test]
fn truncated_reply_and_limit_rejection_stop_immediately() {
    let source = body(70 * 1024);
    let mut calls = 0;
    assert!(
        read_complete(&reference(&source), "bad-frame", source.len(), |request| {
            calls += 1;
            let mut bytes = response(request, &source, |_| {});
            bytes.pop();
            Ok(bytes)
        })
        .is_err()
    );
    assert_eq!(calls, 1);
    let mut calls = 0;
    assert_eq!(
        read_complete(
            &reference(&source),
            "host-budget",
            source.len(),
            |request| {
                calls += 1;
                Reply::new(
                    request,
                    Outcome::Rejected {
                        failure: Failure::Limit,
                    },
                )?
                .encode()
            }
        ),
        Err(Error::Limit)
    );
    assert_eq!(calls, 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn real_original_store_and_host_bridge_read_and_verify_all_seventy_kib() {
    use morrow_agent_content_v1::host::ContentHost;
    use morrow_core::{
        content::CardRecord,
        dispatch::HostRuntime,
        lifecycle::GrantKind,
        store::{EventBudget, Store},
    };
    let source = body(70 * 1024);
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(
        &temp.path().join("complete-body.sqlite"),
        EventBudget::default(),
    )
    .unwrap();
    let card = CardRecord::new(
        "content-card",
        "org.example.note",
        1,
        "whole source",
        source.clone(),
    )
    .unwrap();
    store.create_local("seed-content", &card).unwrap();
    let mut host = HostRuntime::new(store).unwrap();
    let mut connection = host.connect().unwrap();
    host.grant(
        &mut connection,
        GrantKind::ReadContent,
        "content-card",
        100,
        0,
    )
    .unwrap();
    let mut content = ContentHost::new(&host, &connection, vec!["content-card".into()]).unwrap();
    let original = host.store_local().card("content-card").unwrap().unwrap();
    let reference = ContentRef {
        card_id: "content-card".into(),
        revision: original.summary().revision,
        total_length: original.body().len() as u64,
        body_sha256: Sha256::digest(original.body()).into(),
    };
    let mut calls = 0;
    let verified = read_complete(&reference, "real-source", source.len(), |request| {
        calls += 1;
        content.dispatch(&mut host, &connection, &request.encode()?, || 1)
    })
    .unwrap();
    assert_eq!(calls, 3);
    assert_eq!(verified.body(), source);
    assert_eq!(verified.reference(), &reference);
    assert_eq!(
        host.store_local()
            .card("content-card")
            .unwrap()
            .unwrap()
            .encode(),
        original.encode()
    );
    host.store_local().integrity_check().unwrap();
}
