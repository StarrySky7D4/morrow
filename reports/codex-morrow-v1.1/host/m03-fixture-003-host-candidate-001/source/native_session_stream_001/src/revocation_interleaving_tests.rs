//! Deterministic API/owner interleavings. Real new native ledger and poll path,
//! simulated completed-request progress; no child, HTTP request or Core response proof.
use super::*;
use crate::authority;

async fn setup() -> (Http, authority::HostAuthority, mpsc::Receiver<Control>) {
    let parent = authority::simulated_http_parent();
    let shared = Arc::new(Shared {
        created: Instant::now(),
        gate: parent.gate.clone(),
        state: std::sync::Mutex::new(crate::Snapshot {
            session: parent.session,
            epoch: parent.epoch,
            pid: parent.pid,
            generation: 1,
            phase: "Active".into(),
            owner_retained: true,
            exit_code: None,
            exit_observed: false,
            stdout_eof: false,
            stderr_eof: false,
            event_overflow: 0,
            events: vec![],
            http: json!({}),
        }),
    });
    let initial = w::Frame {
        kind: w::Kind::Challenge,
        sequence: 0,
        session: parent.session,
        instance_epoch: parent.epoch,
        revocation_generation: 1,
        child_pid: parent.pid,
        code: 0,
        remaining_ms: 10000,
        nonce: vec![7; 32],
        schema_sha256: w::schema_digest().to_vec(),
        artifact_sha256: parent.approval.artifact_sha256.clone(),
        execution_config_sha256: parent.approval.config_sha256.clone(),
        request_budget: 128,
        capabilities: 3,
        operation_id: parent.approval.operation.as_bytes().to_vec(),
        attempt: 1,
        payload: w::Payload::None,
    };
    let (authority, rx) = authority::authority_with_simulated_session(&parent, shared.clone());
    let mut http = Http::new(
        initial,
        parent,
        shared,
        500,
        #[cfg(feature = "qualification-pipe-fault")]
        None,
    )
    .unwrap();
    finish(&mut http).await; // Real data-thread cancellation/reap/join before RequestClosed.
    // These are supplied state inputs, not a claim of real HTTP/material evidence.
    http.progress.request_closed = true;
    http.progress.http_eof = true;
    http.progress.intent = w::IntentPhase::Observed;
    http.progress.response_material_stored = true;
    (http, authority, rx)
}
async fn finish(http: &mut Http) {
    http.pipe.cancel();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !http.pipe_joined {
            http.pipe_events().unwrap();
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    assert!(!http.progress.worker_started);
    assert_eq!(http.parent.gate.lock().unwrap().ordinal, 0);
}
fn applied(http: &Http) -> usize {
    http.shared
        .state
        .lock()
        .unwrap()
        .events
        .iter()
        .filter(|e| e["event"] == "http_cancel_applied")
        .count()
}

#[tokio::test]
async fn owner_commit_before_generation_publish_does_not_trigger_external_poll() {
    let (mut http, mut authority, mut rx) = setup().await;
    let receipt = http.parent.revoke_native(25).unwrap();
    assert_eq!((receipt.source, receipt.reason), (2, 25));
    assert_eq!(http.shared.state.lock().unwrap().generation, 1);
    // Exact original bug window: row state3 committed, shared generation still1.
    assert!(!authority.poll().await.unwrap());
    assert!(matches!(
        rx.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
    assert!(
        !authority
            .events
            .iter()
            .any(|e| e["event"] == "external_revocation_applied")
    );
    http.cancel_http(25).unwrap();
    assert_eq!(http.progress.error_code, 0);
    assert_eq!(applied(&http), 1);
    assert_eq!(http.progress.intent, w::IntentPhase::Observed);
    finish(&mut http).await;
}

#[tokio::test]
async fn true_external_commit_is_applied_by_real_poll_path() {
    let (mut http, mut authority, mut rx) = setup().await;
    let mut external = authority::HostAuthority::open(&http.parent.root).unwrap();
    let r = external.revoke(&http.parent.approval.id).await.unwrap();
    assert_eq!(r["first_revocation_source"], 1);
    assert_eq!(r["runtime_applied"], false);
    let (poll, ()) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(authority.poll(), async {
            match rx.recv().await.unwrap() {
                Control::Revoke(ack) => {
                    http.cancel_http(19).unwrap();
                    ack.send(()).unwrap();
                }
                _ => panic!("expected external revoke"),
            }
        })
    })
    .await
    .unwrap();
    assert!(!poll.unwrap());
    assert_eq!(http.progress.error_code, 19);
    assert!(
        authority
            .events
            .iter()
            .any(|e| e["event"] == "external_revocation_applied")
    );
    assert!(http.parent.gate.lock().unwrap().revoked);
    assert_eq!(http.progress.intent, w::IntentPhase::Observed);
    assert!(!authority.poll().await.unwrap());
    assert!(rx.try_recv().is_err());
    finish(&mut http).await;
}

#[tokio::test]
async fn external_commit_then_close_uses_durable_19_not_callers_25() {
    let (mut http, mut authority, mut rx) = setup().await;
    let mut external = authority::HostAuthority::open(&http.parent.root).unwrap();
    external.revoke(&http.parent.approval.id).await.unwrap();
    http.cancel_http(25).unwrap(); // Close wins scheduling; external already won commit.
    assert_eq!(http.progress.error_code, 19);
    assert_eq!(applied(&http), 1);
    let record = authority::load_grant(
        &authority::connect(&http.parent.root).unwrap(),
        &http.parent.approval.id,
    )
    .unwrap();
    assert_eq!(
        (record.revocation_source, record.revocation_reason),
        (1, 19)
    );
    http.cancel_http(19).unwrap();
    assert_eq!(applied(&http), 1);
    assert!(!authority.poll().await.unwrap());
    assert!(rx.try_recv().is_err());
    assert_eq!(http.progress.intent, w::IntentPhase::Observed);
    finish(&mut http).await;
}

#[tokio::test]
async fn owner_close_then_late_operator_preserves_first_result_and_ack_snapshot() {
    let (mut http, mut authority, mut rx) = setup().await;
    http.cancel_http(25).unwrap();
    let ack = http.frame(
        w::Kind::State,
        2,
        0,
        w::Payload::Progress(http.progress.clone()),
    );
    let mut external = authority::HostAuthority::open(&http.parent.root).unwrap();
    let result = external.revoke(&http.parent.approval.id).await.unwrap();
    assert_eq!(result["first_revocation_source"], 2);
    assert_eq!(result["first_revocation_reason"], 25);
    http.cancel_http(19).unwrap(); // Redundant command must not change terminal facts.
    assert_eq!(http.progress.error_code, 0);
    assert_eq!(applied(&http), 1);
    assert_eq!(ack.payload, w::Payload::Progress(http.progress.clone()));
    assert!(!authority.poll().await.unwrap());
    assert!(rx.try_recv().is_err());
    finish(&mut http).await;
}

#[tokio::test]
async fn unknown_incomplete_or_nonrevoked_provenance_fails_closed() {
    let (mut http, _, _) = setup().await;
    let db = authority::connect(&http.parent.root).unwrap();
    let original = authority::load_grant(&db, &http.parent.approval.id).unwrap();
    for (state, source, reason) in [(3, 0, 0), (3, 1, 25), (3, 2, 0), (3, 3, 19), (2, 1, 19)] {
        let mut bad = original.clone();
        bad.state = state;
        bad.revocation_source = source;
        bad.revocation_reason = reason;
        authority::save_grant(&db, &bad).unwrap();
        assert!(authority::load_grant(&db, &bad.id).is_err());
    }
    authority::save_grant(&db, &original).unwrap();
    let mut drift = original.clone();
    drift.state = 3;
    drift.revocation_source = 1;
    drift.revocation_reason = 19;
    drift.config_sha256[0] ^= 1;
    authority::save_grant(&db, &drift).unwrap();
    assert!(http.parent.revoke_native(25).unwrap_err().contains("drift"));
    authority::save_grant(&db, &original).unwrap();
    db.execute_batch("PRAGMA user_version=3").unwrap();
    assert!(
        authority::HostAuthority::open(&http.parent.root)
            .err()
            .unwrap()
            .contains("version")
    );
    db.execute_batch("PRAGMA user_version=4").unwrap();
    finish(&mut http).await;
}
