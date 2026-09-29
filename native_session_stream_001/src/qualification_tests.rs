//! Qualification plan boundaries and real same-process pipe I/O; no HTTP or child.
use super::*;
use crate::{
    Admission, LaunchSpec,
    authority::HostAuthority,
    pipe_driver::{Command, Driver, EffectGate, Event, Gate},
};
use morrow_native_pipe_win::{Kind, Pipe};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

fn plan() -> PipeFaultPlan {
    PipeFaultPlan {
        version: 1,
        scenario: "pipe-partial-close".into(),
        nonce: "ab".repeat(32),
        fixture_spec_sha256: "cd".repeat(32),
        target: "first-response-body-chunk".into(),
        prefix_bytes: 12,
        close_trigger: "matched-passive-partial-frame-witness".into(),
    }
}
fn initial() -> wire::Frame {
    wire::Frame {
        kind: wire::Kind::Challenge,
        sequence: 0,
        session: 71,
        instance_epoch: 23,
        revocation_generation: 1,
        child_pid: std::process::id(),
        code: 0,
        remaining_ms: 5000,
        nonce: vec![7; 32],
        schema_sha256: wire::schema_digest().to_vec(),
        artifact_sha256: vec![8; 32],
        execution_config_sha256: vec![9; 32],
        request_budget: 128,
        capabilities: 3,
        operation_id: b"qualification-local-pipe".to_vec(),
        attempt: 1,
        payload: wire::Payload::None,
    }
}
fn target(bound: &BoundPlan) -> CutFrame {
    let mut frame = initial();
    frame.kind = wire::Kind::BodyChunk;
    frame.sequence = 2;
    frame.payload = wire::Payload::Chunk(wire::Chunk {
        offset: 0,
        bytes: vec![0x51; 1024],
    });
    CutFrame::new(bound, frame.encode().unwrap(), Some(1024)).unwrap()
}
fn witness(bound: &BoundPlan, frame: &CutFrame) -> PartialFrameWitness {
    PartialFrameWitness {
        identity_sha256: bound.identity_sha256.clone(),
        nonce: bound.plan.nonce.clone(),
        fixture_spec_sha256: bound.plan.fixture_spec_sha256.clone(),
        marker_sha256: "ef".repeat(32),
        marker_ordinal: 1,
        read_id: 2,
        read_issue_count: 2,
        buffered_bytes: 12,
        declared_payload_bytes: frame.original.len() - 4,
        expected_frame_bytes: frame.original.len(),
        prefix_sha256: frame.prefix_sha256.clone(),
    }
}
fn new_bound() -> BoundPlan {
    BoundPlan::new(plan(), &initial(), Instant::now() + Duration::from_secs(5)).unwrap()
}

#[test]
fn qualification_plan_requires_exact_spec_argument_and_fixed_bounds() {
    let p = plan();
    let args = vec![
        "--fixture-spec-sha256".into(),
        p.fixture_spec_sha256.clone(),
    ];
    assert!(p.validate(&args).is_ok());
    for args in [
        vec![],
        vec!["--fixture-spec-sha256".into(), "ff".repeat(32)],
        vec![
            args[0].clone(),
            args[1].clone(),
            args[0].clone(),
            args[1].clone(),
        ],
    ] {
        assert!(p.validate(&args).is_err());
    }
    for change in [0, 1, 2, 3] {
        let mut invalid = p.clone();
        match change {
            0 => invalid.prefix_bytes = 13,
            1 => invalid.nonce = "00".repeat(32),
            2 => invalid.nonce = "AA".repeat(32),
            _ => invalid.scenario = "authority-deadline".into(),
        }
        assert!(invalid.validate(&args).is_err());
    }
    let mut value = serde_json::to_value(&p).unwrap();
    value["rearm"] = json!(true);
    assert!(serde_json::from_value::<PipeFaultPlan>(value).is_err());
}

fn isolated_spec() -> (PathBuf, LaunchSpec) {
    let base =
        std::env::var_os("MORROW_QUALIFICATION_TEST_ROOT").expect("explicit new local test root");
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).unwrap();
    let root = PathBuf::from(base).join(wire::hex(&nonce));
    std::fs::create_dir(&root).unwrap();
    let cwd = root.join("empty-cwd");
    std::fs::create_dir(&cwd).unwrap();
    let executable = root.join("not-launched.bin");
    std::fs::write(&executable, b"test admission artifact; never launched").unwrap();
    let p = plan();
    (
        root,
        LaunchSpec {
            slot: "qualification-test".into(),
            artifact_sha256: wire::digest(&std::fs::read(&executable).unwrap()),
            executable,
            cwd,
            args: vec!["--fixture-spec-sha256".into(), p.fixture_spec_sha256],
            ttl_ms: 5000,
            handshake_ms: 500,
            frame_ms: 500,
            close_ms: 300,
            request_budget: 128,
            http_origin: "http://127.0.0.1:9/".into(),
        },
    )
}
#[test]
fn qualification_config_binding_preserves_original_created_and_prevents_rearm() {
    let (_, spec) = isolated_spec();
    let mut admission = Admission::authorize(spec).unwrap();
    let created = admission.created;
    let original = admission.config;
    let p = plan();
    let mut preimage = b"Morrow/qualification-pipe-fault/v1\0".to_vec();
    preimage.extend_from_slice(&original);
    preimage.extend_from_slice(&p.canonical());
    admission.bind_pipe_fault(p.clone()).unwrap();
    assert_eq!(admission.config, wire::digest(&preimage));
    assert_ne!(admission.config, original);
    assert_eq!(admission.created, created);
    let fixed = admission.config;
    assert!(admission.bind_pipe_fault(p).is_err());
    assert_eq!(admission.config, fixed);
    assert_eq!(admission.created, created);
}
#[test]
fn qualification_host_approval_is_one_shot_without_spawning_or_sending_http() {
    let (root, spec) = isolated_spec();
    let profile = root.join("profile");
    std::fs::create_dir(&profile).unwrap();
    HostAuthority::initialize(&profile, "qualification-test").unwrap();
    let mut host = HostAuthority::open(&profile).unwrap();
    let id = host
        .approve_with_pipe_fault(
            spec.clone(),
            "fixture",
            "guest",
            "qualification-local-pipe",
            plan(),
        )
        .unwrap();
    let mut second = plan();
    second.nonce = "12".repeat(32);
    assert!(
        host.approve_with_pipe_fault(spec, "fixture", "guest", "qualification-local-pipe", second)
            .is_err()
    );
    assert_eq!(
        host.events
            .iter()
            .filter(|e| e["event"] == "authorize_created")
            .count(),
        1
    );
    assert!(host.snapshot().is_none());
    println!(
        "qualification-approval={}",
        json!({"grant_id":id,"approvals":1,"child_spawned":false,"http_requests":0})
    );
}
#[test]
fn qualification_original_frame_and_witness_bindings_reject_substitution() {
    let bound = new_bound();
    let cut = target(&bound);
    let good = witness(&bound, &cut);
    assert!(cut.validate_witness(&bound, &good).is_ok());
    for field in [
        "identity_sha256",
        "nonce",
        "fixture_spec_sha256",
        "prefix_sha256",
        "marker_sha256",
    ] {
        let mut v = serde_json::to_value(&good).unwrap();
        v[field] = json!("mismatch");
        assert!(
            cut.validate_witness(&bound, &serde_json::from_value(v).unwrap())
                .is_err()
        );
    }
    for (field, value) in [
        ("buffered_bytes", 13),
        ("declared_payload_bytes", 12),
        ("expected_frame_bytes", 12),
        ("marker_ordinal", 0),
        ("marker_ordinal", 129),
        ("read_id", 0),
        ("read_issue_count", 0),
    ] {
        let mut v = serde_json::to_value(&good).unwrap();
        v[field] = json!(value);
        assert!(
            cut.validate_witness(&bound, &serde_json::from_value(v).unwrap())
                .is_err()
        );
    }
    let mut bad = wire::Frame::decode(&cut.original).unwrap();
    bad.sequence = 3;
    assert!(CutFrame::new(&bound, bad.encode().unwrap(), Some(1024)).is_err());
    assert!(CutFrame::new(&bound, cut.original.clone(), Some(1025)).is_err());
    let mut wrong_bound = bound.clone();
    wrong_bound.identity.host_execution_config_sha256 = "ff".repeat(32);
    assert!(CutFrame::new(&wrong_bound, cut.original.clone(), Some(1024)).is_err());
    assert!(BoundPlan::new(plan(), &initial(), Instant::now()).is_err());
}

struct LocalPipe {
    driver: Driver,
    client: Option<Pipe>,
    gate: Gate,
    bound: BoundPlan,
    cut: CutFrame,
    observations: Vec<Value>,
    writes: usize,
    completes: usize,
    reaps: Vec<Value>,
}
impl LocalPipe {
    fn new(with_plan: bool) -> Self {
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).unwrap();
        let locator = format!(r"\\.\pipe\morrow-m03-qualification-{}", wire::hex(&nonce));
        let bound = new_bound();
        let cut = target(&bound);
        let gate = Arc::new(Mutex::new(EffectGate {
            revoked: false,
            deadline: bound.deadline,
            ordinal: 0,
            last_write: 0,
            issued_body_end: 0,
            network_pending: false,
        }));
        let driver = if with_plan {
            Driver::spawn_with_fault(
                locator.clone(),
                std::process::id(),
                gate.clone(),
                bound.clone(),
            )
            .unwrap()
        } else {
            Driver::spawn(locator.clone(), std::process::id(), gate.clone()).unwrap()
        };
        let until = Instant::now() + Duration::from_secs(2);
        let client = loop {
            if let Ok(c) = Pipe::open_client(&locator) {
                break c;
            }
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(1));
        };
        loop {
            if matches!(driver.event(), Some(Event::Connected { .. })) {
                break;
            }
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(1));
        }
        Self {
            driver,
            client: Some(client),
            gate,
            bound,
            cut,
            observations: vec![],
            writes: 0,
            completes: 0,
            reaps: vec![],
        }
    }
    fn drain(&mut self) {
        while let Some(e) = self.driver.event() {
            match e {
                Event::QualificationObservation { event, detail } => self
                    .observations
                    .push(json!({"event":event,"detail":detail})),
                Event::WriteIssued {
                    bytes, body_end, ..
                } => {
                    self.writes += 1;
                    assert_eq!(bytes, 12);
                    assert_eq!(body_end, None);
                }
                Event::WriteCompleted { .. } => self.completes += 1,
                Event::Reaped {
                    kind,
                    id,
                    bytes,
                    error,
                } => self
                    .reaps
                    .push(json!({"kind":format!("{kind:?}"),"id":id,"bytes":bytes,"error":error})),
                _ => {}
            }
        }
    }
    fn read(&mut self, n: usize) -> morrow_native_pipe_win::Completed {
        self.client.as_mut().unwrap().begin_read(n).unwrap();
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            self.drain();
            if let Some(c) = self.client.as_mut().unwrap().poll(Kind::Read).unwrap() {
                return c;
            }
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn prefix(&mut self) {
        self.driver
            .send(Command::PrefixFrame {
                bytes: self.cut.original.clone(),
                body_end: Some(1024),
            })
            .unwrap();
        let four = self.read(4);
        let eight = self.read(8);
        assert_eq!(four.error, None);
        assert_eq!(eight.error, None);
        let mut buffered = four.bytes;
        buffered.extend(eight.bytes);
        assert_eq!(buffered, self.cut.original[..12]);
        assert_eq!(
            wire::payload_length(&buffered[..4]).unwrap() + 4,
            self.cut.original.len()
        );
        assert!(wire::Frame::decode(&buffered).is_err()); // Actual incomplete framing, no decoded BodyChunk.
        let until = Instant::now() + Duration::from_secs(2);
        while !self
            .observations
            .iter()
            .any(|o| o["event"] == "qualification_pipe_prefix_reaped")
        {
            self.drain();
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(self.gate.lock().unwrap().issued_body_end, 0);
        assert_eq!(self.completes, 0);
    }
    fn submit(&mut self, w: PartialFrameWitness) -> Result<Value> {
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        self.driver.close_after_witness(w, tx);
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            self.drain();
            match rx.try_recv() {
                Ok(r) => return r,
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    return Err("owner no longer available".into());
                }
                _ => {}
            }
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn finish(&mut self) -> Option<String> {
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            self.drain();
            if let Some(r) = self.driver.join_if_finished() {
                self.drain();
                return r.unwrap().error;
            }
            assert!(
                Instant::now() < until,
                "qualification real owner join unconfirmed"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
}

#[test]
fn qualification_real_prefix_witness_close_reaps_without_full_frame_credit_or_tail() {
    let mut pipe = LocalPipe::new(true);
    let good = witness(&pipe.bound, &pipe.cut);
    assert!(pipe.submit(good.clone()).is_err()); // Not issued/reaped yet.
    pipe.prefix();
    let mut invalid = good.clone();
    invalid.nonce = "ff".repeat(32);
    assert!(pipe.submit(invalid).is_err());
    assert!(
        pipe.observations
            .iter()
            .all(|o| o["event"] != "qualification_pipe_witness_matched")
    );
    // A pending real read on the server must be cancelled and reaped during close.
    pipe.driver.send(Command::Read(4)).unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    while !pipe
        .observations
        .iter()
        .any(|o| o["event"] == "qualification_pipe_read_issued")
    {
        pipe.drain();
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    let accepted = pipe.submit(good.clone()).unwrap();
    assert_eq!(accepted["data_close_proven"], false);
    assert!(pipe.submit(good).is_err()); // Replay/closed queue never restarts an operation.
    let closed = pipe.finish();
    assert_eq!(closed, None); // No manufactured OS error/revocation reason.
    let eof = pipe.read(4);
    assert!(eof.bytes.is_empty());
    assert!(matches!(eof.error, Some(109 | 232 | 233) | None));
    assert!(!pipe.client.as_ref().unwrap().has_operation(Kind::Read));
    assert_eq!(pipe.writes, 1);
    assert_eq!(pipe.completes, 0);
    let writes: Vec<_> = pipe.reaps.iter().filter(|r| r["kind"] == "Write").collect();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0]["bytes"], 12);
    assert!(writes[0]["error"].is_null());
    assert!(
        pipe.reaps
            .iter()
            .any(|r| r["kind"] == "Read" && r["error"] == 995)
    );
    assert_eq!(pipe.gate.lock().unwrap().issued_body_end, 0);
    assert_eq!(
        pipe.observations
            .iter()
            .filter(|o| o["event"] == "qualification_pipe_witness_matched")
            .count(),
        1
    );
    assert!(
        pipe.observations
            .iter()
            .any(|o| o["event"] == "qualification_pipe_closed"
                && o["detail"]["outcome"]["all_operations_reaped"] == true)
    );
    println!(
        "qualification-real-pipe={}",
        json!({"peer_buffered_bytes":12,"peer_full_frame_decoded":false,"original_complete_frames":pipe.completes,
        "issued_body_end":0,"owner_joined":true,"platform_io_error":closed,"reaps":pipe.reaps,"observations":pipe.observations})
    );
}
#[test]
fn qualification_close_requires_bound_plan() {
    let mut pipe = LocalPipe::new(false);
    assert!(pipe.submit(witness(&pipe.bound, &pipe.cut)).is_err());
    pipe.driver.cancel();
    assert_eq!(pipe.finish(), None);
    assert_eq!(pipe.writes, 0);
}
#[test]
fn qualification_expired_or_revoked_gate_rejects_close_and_never_reissues_tail() {
    for revoked in [false, true] {
        let mut pipe = LocalPipe::new(true);
        pipe.prefix();
        let good = witness(&pipe.bound, &pipe.cut);
        {
            let mut g = pipe.gate.lock().unwrap();
            if revoked {
                g.revoked = true;
            } else {
                g.deadline = Instant::now();
            }
        }
        assert!(pipe.submit(good).is_err());
        assert_eq!(pipe.finish(), None);
        assert_eq!(pipe.writes, 1);
        assert_eq!(pipe.completes, 0);
        assert_eq!(pipe.gate.lock().unwrap().issued_body_end, 0);
        assert!(
            pipe.observations
                .iter()
                .all(|o| o["event"] != "qualification_pipe_witness_matched")
        );
    }
}

#[test]
fn qualification_peer_disconnect_retains_actual_error_separately_from_join() {
    let mut pipe = LocalPipe::new(true);
    pipe.prefix();
    drop(pipe.client.take());
    pipe.driver.send(Command::Read(4)).unwrap();
    let error = pipe.finish();
    assert!(error.is_some());
    assert!(
        pipe.reaps
            .iter()
            .any(|r| r["kind"] == "Read" && matches!(r["error"].as_u64(), Some(109 | 232 | 233)))
    );
    assert_eq!(pipe.writes, 1);
    assert_eq!(pipe.completes, 0);
    assert!(
        pipe.observations
            .iter()
            .any(|o| o["event"] == "qualification_pipe_closed"
                && !o["detail"]["outcome"]["platform_io_error"].is_null())
    );
    println!(
        "qualification-peer-error={}",
        json!({"retained_error":error,"owner_joined":true,"original_complete_frames":0,"reaps":pipe.reaps})
    );
}

#[test]
fn qualification_new_write_after_cut_is_rejected_before_os_issue() {
    let mut pipe = LocalPipe::new(true);
    pipe.prefix();
    pipe.driver
        .send(Command::Write {
            bytes: pipe.cut.original.clone(),
            body_end: Some(1024),
        })
        .unwrap();
    assert_eq!(
        pipe.finish().as_deref(),
        Some("qualification no new write/tail after prefix")
    );
    assert_eq!(pipe.writes, 1);
    assert_eq!(pipe.completes, 0);
    assert_eq!(pipe.gate.lock().unwrap().issued_body_end, 0);
    assert_eq!(pipe.gate.lock().unwrap().last_write, 1);
}
