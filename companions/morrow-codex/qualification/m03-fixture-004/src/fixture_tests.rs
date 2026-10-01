//! Synthetic local tests only. No native executable, Core model or HTTP launched.
use super::*;
use crate::driver::{CancelReason, CancellationSignal};
use std::path::PathBuf;
static NEXT: AtomicU64 = AtomicU64::new(0);
pub(crate) fn spec() -> Spec {
    Spec {
        version: 2,
        mode: Mode::PassiveObserve,
        scenario: Scenario::NetworkAbort,
        nonce: "a".repeat(64),
        request_limit: 32768,
        response_limit: 65536,
        consumer_events: 8,
        credit_limit: 16384,
        pipe_buffer: 1024,
        max_chunk: 1024,
        deadline_policy: "retain-original".into(),
        events_file: "fixture-events.jsonl".into(),
        evidence_queue_capacity: 128,
        evidence_record_limit: 128,
        evidence_record_limit_bytes: 8192,
        evidence_file_limit_bytes: 1048576,
        pipe_prefix_bytes: None,
    }
}
pub(crate) fn fixture() -> (Arc<Fixture>, PathBuf) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "m03-fixture003-local-{stamp}-{}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let bytes = serde_json::to_vec(&spec()).unwrap();
    std::fs::write(root.join("fixture-spec.json"), &bytes).unwrap();
    let f = Fixture::open(&root, &format!("{:x}", Sha256::digest(&bytes))).unwrap();
    f.bind(Identity {
        session: 9,
        epoch: 1,
        child_pid: std::process::id(),
        attempt: 1,
        operation_id_sha256: "b".repeat(64),
        host_execution_config_sha256: "c".repeat(64),
    })
    .unwrap();
    (f, root)
}
#[test]
fn strict_v2_spec_rejects_old_modes_unknown_fields_missing_null_and_widened_bounds() {
    let value = serde_json::to_value(spec()).unwrap();
    for scenario in [
        Scenario::AuthorityDeadline,
        Scenario::NetworkAbort,
        Scenario::PipePartialClose,
    ] {
        let mut s = spec();
        s.scenario = scenario;
        s.pipe_prefix_bytes = if s.scenario == Scenario::PipePartialClose {
            Some(12)
        } else {
            None
        };
        s.validate().unwrap();
    }
    for (key, bad) in [
        ("version", json!(1)),
        ("mode", json!("core-revoke")),
        ("mode", json!("data-pending")),
        ("scenario", json!("unknown")),
        ("nonce", json!("0".repeat(64))),
        ("nonce", json!("A".repeat(64))),
        ("request_limit", json!(65536)),
        ("response_limit", json!(131072)),
        ("consumer_events", json!(9)),
        ("credit_limit", json!(32768)),
        ("pipe_buffer", json!(2048)),
        ("max_chunk", json!(2048)),
        ("deadline_policy", json!("renew")),
        ("events_file", json!("../events")),
        ("evidence_queue_capacity", json!(129)),
        ("evidence_record_limit", json!(129)),
        ("evidence_record_limit_bytes", json!(8193)),
        ("evidence_file_limit_bytes", json!(1048577)),
        ("pipe_prefix_bytes", json!(12)),
        ("release_file", json!("release-consumer.json")),
    ] {
        let mut v = value.clone();
        v[key] = bad;
        assert!(
            match serde_json::from_value::<Spec>(v) {
                Err(_) => true,
                Ok(s) => s.validate().is_err(),
            },
            "accepted {key}"
        );
    }
    for key in value.as_object().unwrap().keys() {
        let mut v = value.clone();
        v.as_object_mut().unwrap().remove(key);
        assert!(
            serde_json::from_value::<Spec>(v).is_err(),
            "missing required {key}"
        );
    }
}
#[tokio::test]
async fn exact_byte_digest_duplicate_binding_and_evidence_replay_are_rejected() {
    let (f, root) = fixture();
    assert!(Fixture::open(&root, &"f".repeat(64)).is_err());
    let bytes = std::fs::read(root.join("fixture-spec.json")).unwrap();
    assert!(Fixture::open(&root, &format!("{:x}", Sha256::digest(&bytes))).is_err());
    let identity = f.inner.state.lock().unwrap().identity.clone().unwrap();
    assert!(f.bind(identity).is_err());
    let first = Instant::now();
    let expiry = first + Duration::from_secs(1);
    f.bind_authority(first, expiry, 1000).unwrap();
    let initial = f.authority_observation();
    assert!(
        f.bind_authority(first, expiry + Duration::from_secs(10), 11000)
            .is_err()
    );
    assert_eq!(f.authority_observation(), initial);
    assert_eq!(
        initial["original_deadline_offset_ns"].as_u64().unwrap()
            - initial["first_read_offset_ns"].as_u64().unwrap(),
        1_000_000_000
    );
    f.request_stop();
    assert!(
        f.join_until(tokio::time::Instant::now() + Duration::from_secs(1))
            .await
    );
}
#[tokio::test]
async fn failed_disk_write_retains_failure_but_owned_writer_can_really_join() {
    let (f, root) = fixture();
    let bad = Fixture::with_writer(
        spec(),
        "d".repeat(64),
        File::open(root.join("fixture-spec.json")).unwrap(),
    )
    .unwrap();
    bad.bind(f.inner.state.lock().unwrap().identity.clone().unwrap())
        .unwrap();
    bad.emit("local", json!({}));
    tokio::time::timeout(Duration::from_secs(1), bad.inner.failed.cancelled())
        .await
        .unwrap();
    bad.request_stop();
    assert!(
        bad.join_until(tokio::time::Instant::now() + Duration::from_secs(1))
            .await
    );
    assert_eq!(bad.snapshot()["evidence_complete"], false);
    assert_eq!(bad.snapshot()["writer_joined"], true);
    f.request_stop();
    assert!(
        f.join_until(tokio::time::Instant::now() + Duration::from_secs(1))
            .await
    );
}
#[tokio::test]
async fn concurrent_markers_are_bound_ordered_and_once_observations_do_not_hold() {
    let (f, root) = fixture();
    let mut producers = Vec::new();
    for _ in 0..2 {
        let f = f.clone();
        producers.push(thread::spawn(move || {
            for _ in 0..10 {
                f.emit("local_concurrent", json!({}));
            }
        }));
    }
    for h in producers {
        while !h.is_finished() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        h.join().unwrap();
    }
    f.emit_once("local_once", json!({"first":true}));
    f.emit_once("local_once", json!({"first":false}));
    f.request_stop();
    assert!(
        f.join_until(tokio::time::Instant::now() + Duration::from_secs(1))
            .await
    );
    let text = std::fs::read_to_string(root.join("fixture-events.jsonl")).unwrap();
    assert_eq!(text.lines().count(), 21);
    for (i, line) in text.lines().enumerate() {
        let v: Value = serde_json::from_str(line).unwrap();
        assert_eq!(v["ordinal"], (i + 1) as u64);
        assert_eq!(v["scenario"], "network-abort");
        assert_eq!(v["identity"]["session"], 9);
    }
    assert_eq!(f.snapshot()["evidence_complete"], true);
    assert!(f.snapshot().get("stages_complete").is_none());
}
#[tokio::test]
async fn overflow_and_record_limits_fail_without_blocking_or_clean_receipt() {
    let (f, _) = fixture();
    let (tx, _rx) = mpsc::sync_channel(1);
    let blocked = Fixture {
        inner: f.inner.clone(),
        tx,
        writer: Mutex::new(None),
    };
    blocked.emit("first", json!({}));
    blocked.emit("overflow", json!({}));
    assert!(blocked.failed());
    assert_eq!(blocked.snapshot()["enqueued_records"], 1);
    assert_eq!(blocked.snapshot()["evidence_complete"], false);
    f.request_stop();
    assert!(
        f.join_until(tokio::time::Instant::now() + Duration::from_secs(1))
            .await
    );
    let (f, _) = fixture();
    f.emit("too_large", json!({"data":"x".repeat(8192)}));
    assert!(f.failed());
    assert_eq!(f.snapshot()["enqueued_records"], 0);
    f.request_stop();
    assert!(
        f.join_until(tokio::time::Instant::now() + Duration::from_secs(1))
            .await
    );
    let (f, _) = fixture();
    for _ in 0..128 {
        f.emit("limit", json!({}));
    }
    f.emit("over_limit", json!({}));
    assert!(f.failed());
    f.request_stop();
    assert!(
        f.join_until(tokio::time::Instant::now() + Duration::from_secs(1))
            .await
    );
    assert_eq!(f.snapshot()["enqueued_records"], 128);
    assert_eq!(f.snapshot()["written_records"], 128);
}
#[tokio::test]
async fn an_expired_shared_deadline_retains_the_writer_handle_until_actual_join() {
    let (f, _) = fixture();
    f.request_stop();
    assert!(!f.join_until(tokio::time::Instant::now()).await);
    assert!(f.writer.lock().unwrap().is_some());
    assert_eq!(f.snapshot()["writer_joined"], false);
    // Local test teardown, not a runtime budget extension or an authority grant.
    assert!(
        f.join_until(tokio::time::Instant::now() + Duration::from_secs(1))
            .await
    );
}
#[test]
fn cancellation_observation_keeps_actual_first_reason_for_deadline_host_and_protocol_orders() {
    for (first, second) in [
        (CancelReason::Deadline, CancelReason::HostCancelled),
        (CancelReason::HostCancelled, CancelReason::Deadline),
        (CancelReason::NativeFailure, CancelReason::HostCancelled),
        (CancelReason::HostCancelled, CancelReason::NativeFailure),
    ] {
        let signal = CancellationSignal::default();
        signal.cancel(first);
        signal.cancel_observed_at(second, "native_control");
        let (records, overflow) = signal.take_observations();
        assert!(!overflow);
        assert_eq!(records.len(), 2);
        assert_eq!(signal.reason(), Some(first));
        assert!(!records[0].transition.cancel_gate_before);
        assert_eq!(records[0].transition.first_cancel_reason_after, Some(first));
        assert_eq!(
            records[1].transition.first_cancel_reason_before,
            Some(first)
        );
        assert_eq!(records[1].transition.first_cancel_reason_after, Some(first));
        assert!(!records[1].transition.host_control_closed_gate);
        assert!(!signal.is_paused());
        assert_eq!(records[0].observation_ordinal, 1);
        assert_eq!(records[1].observation_ordinal, 2);
    }
}

#[tokio::test]
async fn finished_writer_is_reaped_with_no_remaining_wait_budget() {
    let (f, _root) = fixture();
    f.request_stop();
    let limit = tokio::time::Instant::now() + Duration::from_secs(1);
    while !f.writer.lock().unwrap().as_ref().unwrap().is_finished() {
        assert!(tokio::time::Instant::now() < limit);
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert!(f.join_until(tokio::time::Instant::now()).await);
    assert_eq!(f.snapshot()["writer_joined"], true);
    assert!(f.writer.lock().unwrap().is_none());
}

#[tokio::test]
async fn missing_ack_does_not_starve_owned_evidence_join_in_shared_budget() {
    let (f, _root) = fixture();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(100);
    let (ack, joined) = tokio::join!(
        tokio::time::timeout_at(deadline, std::future::pending::<()>()),
        async { f.request_stop(); f.join_until(deadline).await },
    );
    assert!(ack.is_err(), "missing ACK remains unconfirmed");
    assert!(joined, "the actual evidence thread must be independently joined");
    assert_eq!(f.snapshot()["evidence_complete"], true);
}
