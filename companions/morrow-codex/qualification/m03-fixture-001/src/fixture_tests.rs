//! Local fixtures only; none of these tests invoke a Core model or HTTP.
use super::*;
#[tokio::test]
async fn failed_disk_write_remains_failure_while_owner_can_stop_and_join() {
    let (f, root) = fixture();
    let bad = Fixture::with_writer(
        spec(),
        "d".repeat(64),
        File::open(root.join("fixture-spec.json")).unwrap(),
        root.join("absent-release.json"),
    )
    .unwrap();
    bad.bind(f.inner.state.lock().unwrap().identity.clone().unwrap())
        .unwrap();
    bad.emit("local_write_failure", json!({}));
    tokio::time::timeout(Duration::from_secs(1), bad.inner.failed.cancelled())
        .await
        .unwrap();
    bad.request_stop();
    assert!(
        bad.join_until(tokio::time::Instant::now() + Duration::from_secs(1))
            .await
    );
    assert_eq!(bad.snapshot()["evidence_complete"], false);
    f.request_stop();
    assert!(
        f.join_until(tokio::time::Instant::now() + Duration::from_secs(1))
            .await
    );
}
#[tokio::test]
async fn concurrent_marker_producers_preserve_sequence_without_disk_under_gate() {
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
    for producer in producers {
        while !producer.is_finished() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        producer.join().unwrap();
    }
    f.request_stop();
    assert!(
        f.join_until(tokio::time::Instant::now() + Duration::from_secs(1))
            .await
    );
    let text = std::fs::read_to_string(root.join("fixture-events.jsonl")).unwrap();
    assert_eq!(text.lines().count(), 20);
    for (index, line) in text.lines().enumerate() {
        assert_eq!(
            serde_json::from_str::<Value>(line).unwrap()["ordinal"],
            (index + 1) as u64
        );
    }
    assert_eq!(f.snapshot()["evidence_complete"], true);
}
static NEXT: AtomicU64 = AtomicU64::new(0);
fn spec() -> Spec {
    Spec {
        version: 1,
        mode: Mode::CoreRevoke,
        nonce: "a".repeat(64),
        request_limit: 32768,
        response_limit: 65536,
        consumer_events: 8,
        credit_limit: 16384,
        pipe_buffer: 1024,
        max_chunk: 1024,
        min_pending_samples: 3,
        min_sample_interval_ms: 25,
        revoke_ack_limit_ms: 500,
        events_file: "fixture-events.jsonl".into(),
        release_file: "release-consumer.json".into(),
    }
}
pub(crate) fn fixture() -> (Arc<Fixture>, PathBuf) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "m03-fixture-local-{stamp}-{}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let bytes = serde_json::to_vec(&spec()).unwrap();
    std::fs::write(root.join("fixture-spec.json"), &bytes).unwrap();
    let fixture = Fixture::open(&root, &format!("{:x}", Sha256::digest(&bytes))).unwrap();
    fixture
        .bind(Identity {
            session: 9,
            epoch: 1,
            child_pid: std::process::id(),
            attempt: 1,
            operation_id_sha256: "b".repeat(64),
            host_execution_config_sha256: "c".repeat(64),
        })
        .unwrap();
    (fixture, root)
}
fn release_value(fixture: &Fixture, event: &CoreEventId) -> Value {
    json!({"version":1,"mode":"core-revoke","nonce":fixture.inner.spec.nonce,
        "spec_sha256":fixture.inner.spec_sha256,"identity":fixture.inner.state.lock().unwrap().identity,
        "stage":"release-consumer","event":event})
}
pub(crate) fn release(fixture: &Fixture, event: &CoreEventId) -> Result<(), &'static str> {
    fixture
        .inner
        .release(&serde_json::to_vec(&release_value(fixture, event)).unwrap())
}
#[test]
fn strict_spec_rejects_wider_limits_and_unknown_controls() {
    let mut s = spec();
    s.validate().unwrap();
    s.credit_limit = 32768;
    assert!(s.validate().is_err());
    let mut value = serde_json::to_value(spec()).unwrap();
    value["execute_command"] = json!("not permitted");
    assert!(serde_json::from_value::<Spec>(value).is_err());
    let mut s = spec();
    s.release_file = "../release.json".into();
    assert!(s.validate().is_err());
}
#[tokio::test]
async fn release_requires_same_event_and_identity_after_gate_close_and_is_once_only() {
    let (f, _) = fixture();
    let id = CoreEventId::delta(1, 1, "local test only");
    assert!(release(&f, &id).is_err());
    {
        let mut s = f.inner.state.lock().unwrap();
        s.held = Some(id.clone());
        s.queued_a = true;
        s.reserved_b = true;
    }
    f.inner.gate_closed.store(true, Ordering::Release);
    let mut wrong = release_value(&f, &id);
    wrong["identity"]["attempt"] = json!(2);
    assert!(
        f.inner
            .release(&serde_json::to_vec(&wrong).unwrap())
            .is_err()
    );
    assert!(release(&f, &CoreEventId::delta(2, 1, "other event")).is_err());
    release(&f, &id).unwrap();
    assert!(release(&f, &id).is_err());
    f.request_stop();
    assert!(
        f.join_until(tokio::time::Instant::now() + Duration::from_secs(1))
            .await
    );
}
#[tokio::test]
async fn full_evidence_queue_fails_nonblocking_and_join_wait_is_bounded() {
    let (f, _) = fixture();
    assert!(!f.join_until(tokio::time::Instant::now()).await);
    let (tx, _rx) = mpsc::sync_channel(1);
    let blocked = Fixture {
        inner: f.inner.clone(),
        tx,
        writer: Mutex::new(None),
    };
    blocked.emit("local_first", json!({}));
    blocked.emit("local_overflow", json!({}));
    assert!(blocked.failed());
    assert_eq!(blocked.snapshot()["evidence_complete"], false);
    f.request_stop();
    assert!(
        f.join_until(tokio::time::Instant::now() + Duration::from_secs(1))
            .await
    );
}
#[tokio::test]
async fn disk_records_flush_and_atomic_release_reaches_waiter_without_gate_lock() {
    let (f, root) = fixture();
    let id = CoreEventId::delta(1, 1, "local fixture disk test");
    f.queued(&id);
    {
        let mut s = f.inner.state.lock().unwrap();
        s.held = Some(id.clone());
        s.reserved_b = true;
    }
    f.observe_revoke(json!({"local_test":true}), true);
    let r = release_value(&f, &id);
    std::fs::write(root.join("release.tmp"), serde_json::to_vec(&r).unwrap()).unwrap();
    std::fs::rename(root.join("release.tmp"), root.join("release-consumer.json")).unwrap();
    tokio::time::timeout(Duration::from_secs(1), f.inner.released.cancelled())
        .await
        .unwrap();
    f.request_stop();
    assert!(
        f.join_until(tokio::time::Instant::now() + Duration::from_secs(1))
            .await
    );
    assert_eq!(f.snapshot()["evidence_complete"], true);
    let lines = std::fs::read_to_string(root.join("fixture-events.jsonl")).unwrap();
    assert_eq!(lines.lines().count(), 3);
    for line in lines.lines() {
        let v: Value = serde_json::from_str(line).unwrap();
        assert_eq!(v["identity"]["session"], 9);
    }
}
