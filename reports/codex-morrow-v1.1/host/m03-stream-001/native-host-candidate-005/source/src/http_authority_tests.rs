//! New adapter API checks with simulated owner rows. No process or network calls.
use super::*;
fn proposal(parent: &Parent) -> Proposal {
    let initial = wire::Frame {
        kind: wire::Kind::Challenge,
        sequence: 0,
        session: parent.session,
        instance_epoch: parent.epoch,
        revocation_generation: 1,
        child_pid: parent.pid,
        code: 0,
        remaining_ms: 10000,
        nonce: vec![1; 32],
        schema_sha256: wire::schema_digest().to_vec(),
        artifact_sha256: parent.approval.artifact_sha256.clone(),
        execution_config_sha256: parent.approval.config_sha256.clone(),
        request_budget: 128,
        capabilities: 3,
        operation_id: parent.approval.operation.as_bytes().to_vec(),
        attempt: 1,
        payload: wire::Payload::None,
    };
    let body = b"synthetic".to_vec();
    parent
        .proposal(
            &initial,
            wire::Prepare {
                method: "POST".into(),
                absolute_target: "http://127.0.0.1:1/v1/responses".into(),
                headers: vec![],
                body_bytes: body.len() as u32,
                body_sha256: wire::digest(&body).to_vec(),
                response_limit_bytes: 1024,
            },
            body,
        )
        .unwrap()
}
fn approved(parent: &Parent) -> NativeHttpGrant {
    let p = proposal(parent);
    let d = p.decision.clone();
    NativeHttpGrant::approve(parent.clone(), p, &d.proposal_ref, &d.request_sha256, 1024).unwrap()
}
#[tokio::test]
async fn consumed_native_grant_cannot_repeat_after_existing_core_history() {
    let parent = authority::simulated_http_parent();
    let mut grant = approved(&parent);
    let prepared = Record::prepared(grant.command.clone()).unwrap();
    parent
        .store
        .lock()
        .unwrap()
        .append_io_intent_local_authorized(&prepared, || Ok(()))
        .unwrap();
    let decision = grant.decision().clone();
    assert!(
        grant
            .claim(&decision, CancellationToken::new())
            .err()
            .unwrap()
            .contains("never resend")
    );
    assert_eq!(
        load(&authority::connect(&parent.root).unwrap(), &grant.record.id)
            .unwrap()
            .state,
        2
    );
    assert!(
        grant
            .claim(&decision, CancellationToken::new())
            .err()
            .unwrap()
            .contains("consumed")
    );
    assert_eq!(grant.intent_phase().unwrap(), wire::IntentPhase::Prepared);
    assert_eq!(parent.gate.lock().unwrap().ordinal, 0);
    assert!(!parent.gate.lock().unwrap().network_pending);
}
#[test]
fn approval_checks_owner_identity_and_original_deadline() {
    let parent = authority::simulated_http_parent();
    let db = authority::connect(&parent.root).unwrap();
    let original = authority::load_owner(&db).unwrap().unwrap();
    for field in 0..4 {
        let mut owner = original.clone();
        match field {
            0 => owner.child_pid += 1,
            1 => owner.session += 1,
            2 => owner.epoch += 1,
            _ => owner.phase = "Closing".into(),
        };
        authority::save_owner(&db, &owner).unwrap();
        let p = proposal(&parent);
        let d = p.decision.clone();
        assert!(
            NativeHttpGrant::approve(parent.clone(), p, &d.proposal_ref, &d.request_sha256, 1024)
                .err()
                .is_some()
        );
    }
    authority::save_owner(&db, &original).unwrap();
    // Deterministic expired absolute deadline, never a fresh duration on approve.
    parent.gate.lock().unwrap().deadline = Instant::now() - Duration::from_millis(1);
    let p = proposal(&parent);
    let d = p.decision.clone();
    assert!(
        NativeHttpGrant::approve(parent.clone(), p, &d.proposal_ref, &d.request_sha256, 1024)
            .err()
            .unwrap()
            .contains("expired")
    );
    let count: i64 = db
        .query_row("SELECT count(*) FROM http_approvals", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}
#[tokio::test]
async fn commit_binds_full_decision_and_expired_commit_does_not_consume() {
    let parent = authority::simulated_http_parent();
    let mut grant = approved(&parent);
    let original = grant.decision().clone();
    for field in 0..4 {
        let mut d = original.clone();
        match field {
            0 => d.http_grant_ref[0] ^= 1,
            1 => d.request_sha256[0] ^= 1,
            2 => d.endpoint_ref[0] ^= 1,
            _ => d.response_limit_bytes -= 1,
        };
        assert!(grant.claim(&d, CancellationToken::new()).is_err());
    }
    parent.gate.lock().unwrap().deadline = Instant::now() - Duration::from_millis(1);
    assert!(
        grant
            .claim(&original, CancellationToken::new())
            .err()
            .unwrap()
            .contains("expired")
    );
    assert_eq!(
        load(&authority::connect(&parent.root).unwrap(), &grant.record.id)
            .unwrap()
            .state,
        1
    );
    assert_eq!(grant.intent_phase().unwrap(), wire::IntentPhase::Absent);
}
#[tokio::test]
async fn ticket_owns_pending_interval_and_revoke_prevents_another_fence() {
    let parent = authority::simulated_http_parent();
    let mut grant = approved(&parent);
    let d = grant.decision().clone();
    let ticket = grant.claim(&d, CancellationToken::new()).unwrap();
    assert_eq!(grant.intent_phase().unwrap(), wire::IntentPhase::Unknown);
    assert!(parent.gate.lock().unwrap().network_pending);
    assert!(grant.claim(&d, CancellationToken::new()).is_err());
    parent.gate.lock().unwrap().revoked = true;
    assert!(network_ticket(&parent.gate).is_err());
    drop(ticket); // Local installation failure/drop must clear pending, not Unknown.
    assert!(!parent.gate.lock().unwrap().network_pending);
    assert_eq!(grant.intent_phase().unwrap(), wire::IntentPhase::Unknown);
    assert!(network_ticket(&parent.gate).is_err());
}
#[tokio::test]
async fn durable_observed_material_remains_observed_after_native_revoke() {
    let parent = authority::simulated_http_parent();
    let mut grant = approved(&parent);
    let d = grant.decision().clone();
    drop(grant.claim(&d, CancellationToken::new()).unwrap());
    // API-only supplied bytes; this is not proof of real HTTP EOF.
    grant
        .observe(
            &wire::Head {
                status: 200,
                headers: vec![],
                remote_address: "127.0.0.1:1".into(),
            },
            b"complete",
        )
        .unwrap();
    parent.revoke_native(19).unwrap();
    grant.revoke().unwrap();
    assert_eq!(grant.intent_phase().unwrap(), wire::IntentPhase::Observed);
    assert!(grant.claim(&d, CancellationToken::new()).is_err());
}
