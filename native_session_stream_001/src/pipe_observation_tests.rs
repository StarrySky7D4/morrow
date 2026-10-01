//! Real local pipe operations; includes explicit same-user child connector; no Core/HTTP.
use super::*;

fn collect(driver: &Driver, observations: &mut Vec<Value>) {
    while let Some(event) = driver.event() {
        if let Event::OwnerObservation(value) = event {
            observations.push(value);
        }
    }
}
fn finish(driver: &mut Driver, observations: &mut Vec<Value>) {
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        collect(driver, observations);
        if let Some(result) = driver.join_if_finished() {
            assert!(result.unwrap().error.is_none());
            collect(driver, observations);
            return;
        }
        assert!(Instant::now() < until, "owner join unconfirmed");
        thread::sleep(Duration::from_millis(1));
    }
}
fn exercise(samples: usize) -> Vec<Value> {
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).unwrap();
    let locator = format!(
        r"\\.\pipe\morrow-m03-observation-{}",
        crate::wire::hex(&nonce)
    );
    let gate = Arc::new(Mutex::new(EffectGate {
        revoked: false,
        deadline: Instant::now() + Duration::from_secs(5),
        ordinal: 0,
        last_write: 0,
        issued_body_end: 0,
        network_pending: false,
    }));
    let mut driver = Driver::spawn(locator.clone(), std::process::id(), gate).unwrap();
    let until = Instant::now() + Duration::from_secs(1);
    let client = loop {
        if let Ok(client) = Pipe::open_client(&locator) {
            break client;
        }
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    };
    let mut connected = false;
    while !connected {
        while let Some(event) = driver.event() {
            if matches!(event, Event::Connected { .. }) {
                connected = true;
            }
        }
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    // Peer owns the pipe but never issues a ReadFile. Existing 1024 buffer,
    // one 8192-byte write, below the native 16KiB credit ceiling.
    assert!(
        driver
            .send(Command::Write {
                bytes: vec![7; 8192],
                body_end: Some(8192)
            })
            .is_ok()
    );
    let mut observations = vec![];
    loop {
        collect(&driver, &mut observations);
        if observations
            .iter()
            .filter(|o| o["stage"] == "incomplete_sample")
            .count()
            >= samples
        {
            break;
        }
        assert!(Instant::now() < until, "actual pending target not reached");
        thread::sleep(Duration::from_millis(1));
    }
    driver.cancel();
    finish(&mut driver, &mut observations);
    drop(client);
    println!("owner-observations={}", json!(observations));
    observations
}
#[test]
fn same_real_operation_sampled_then_probed_and_reaped_on_cancel() {
    let observations = exercise(3);
    let issue = observations
        .iter()
        .find(|o| o["stage"] == "write_issue")
        .unwrap();
    assert_eq!(issue["outcome"]["kind"], "io_pending");
    let samples: Vec<_> = observations
        .iter()
        .filter(|o| o["stage"] == "incomplete_sample")
        .collect();
    assert_eq!(samples.len(), 3);
    for pair in samples.windows(2) {
        assert!(
            pair[1]["before_ns"].as_u64().unwrap() - pair[0]["after_ns"].as_u64().unwrap()
                >= 25_000_000
        );
    }
    let probe = observations
        .iter()
        .find(|o| o["stage"] == "cancel_probe")
        .unwrap();
    assert_eq!(probe["outcome"]["kind"], "incomplete");
    assert_eq!(probe["operation"]["incomplete_samples"], 3);
    let reap = observations
        .iter()
        .find(|o| o["stage"] == "write_reaped")
        .unwrap();
    assert_eq!(reap["outcome"]["error"], 995);
    for o in samples.into_iter().chain([probe, reap]) {
        assert_eq!(o["operation"]["id"], issue["operation"]["id"]);
        assert_eq!(
            o["operation"]["issue_ordinal"],
            issue["operation"]["issue_ordinal"]
        );
        assert_eq!(o["clock_domain"], issue["clock_domain"]);
        assert!(o["before_ns"].as_u64().unwrap() <= o["after_ns"].as_u64().unwrap());
    }
}
#[test]
fn early_cancel_never_manufactures_three_samples() {
    let observations = exercise(1);
    let samples = observations
        .iter()
        .filter(|o| o["stage"] == "incomplete_sample")
        .count();
    assert!(samples < 3);
    let probe = observations
        .iter()
        .find(|o| o["stage"] == "cancel_probe")
        .unwrap();
    assert!(probe["operation"]["incomplete_samples"].as_u64().unwrap() < 3);
    assert!(observations.iter().any(|o| o["stage"] == "write_reaped"));
}
#[test]
fn completed_probe_is_distinct_from_pending_or_unknown() {
    let completed = Ok(Some(morrow_native_pipe_win::Completed {
        id: 9,
        kind: Kind::Write,
        transferred: 12,
        error: None,
        bytes: vec![],
    }));
    assert_eq!(poll_outcome(&completed)["kind"], "completed");
    assert_eq!(poll_outcome(&Ok(None))["kind"], "incomplete");
    assert_eq!(
        poll_outcome(&Err(std::io::Error::from_raw_os_error(6)))["kind"],
        "unconfirmed_error"
    );
}

#[test]
fn peer_observed_prefix_then_cancel_never_completes_or_reissues_frame() {
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).unwrap();
    let locator = format!(r"\\.\pipe\morrow-m03-prefix-{}", crate::wire::hex(&nonce));
    let gate = Arc::new(Mutex::new(EffectGate {
        revoked: false,
        deadline: Instant::now() + Duration::from_secs(5),
        ordinal: 0,
        last_write: 0,
        issued_body_end: 0,
        network_pending: false,
    }));
    let mut driver = Driver::spawn(locator.clone(), std::process::id(), gate).unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    let mut client = loop {
        if let Ok(client) = Pipe::open_client(&locator) {
            break client;
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
    client.begin_read(1024).unwrap();
    assert!(
        driver
            .send(Command::Write {
                bytes: vec![0x53; 8192],
                body_end: Some(8192)
            })
            .is_ok()
    );
    let mut observations = Vec::new();
    let mut complete_frames = 0;
    let peer_prefix = loop {
        while let Some(event) = driver.event() {
            match event {
                Event::OwnerObservation(v) => observations.push(v),
                Event::WriteCompleted { .. } => complete_frames += 1,
                _ => {}
            }
        }
        if let Some(done) = client.poll(Kind::Read).unwrap() {
            break done;
        }
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(peer_prefix.error, None);
    assert_eq!(peer_prefix.bytes, vec![0x53; 1024]);
    // The peer now stops reading. This is actual prefix exposure, not a claim
    // that GetOverlappedResult must report a nonzero error-completion byte count.
    while !observations
        .iter()
        .any(|o| o["stage"] == "incomplete_sample")
    {
        while let Some(event) = driver.event() {
            match event {
                Event::OwnerObservation(v) => observations.push(v),
                Event::WriteCompleted { .. } => complete_frames += 1,
                _ => {}
            }
        }
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    driver.cancel();
    loop {
        while let Some(event) = driver.event() {
            match event {
                Event::OwnerObservation(v) => observations.push(v),
                Event::WriteCompleted { .. } => complete_frames += 1,
                _ => {}
            }
        }
        if let Some(done) = driver.join_if_finished() {
            assert!(done.unwrap().error.is_none());
            while let Some(event) = driver.event() {
                match event {
                    Event::OwnerObservation(v) => observations.push(v),
                    Event::WriteCompleted { .. } => complete_frames += 1,
                    _ => {}
                }
            }
            break;
        }
        assert!(Instant::now() < until, "prefix case owner join unconfirmed");
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(complete_frames, 0);
    assert_eq!(
        observations
            .iter()
            .filter(|o| o["stage"] == "write_issue")
            .count(),
        1
    );
    assert!(
        observations
            .iter()
            .any(|o| o["stage"] == "cancel_probe" && o["outcome"]["kind"] == "incomplete")
    );
    assert!(
        observations
            .iter()
            .any(|o| o["stage"] == "write_reaped" && o["outcome"]["error"] == 995)
    );
    assert!(!client.has_operation(Kind::Read));
    println!(
        "prefix_case={}",
        json!({"peer_observed_bytes":1024,"full_frame_bytes":8192,
        "complete_frames":complete_frames,"owner_joined":true,"observations":observations})
    );
}

#[test]
fn actual_completed_write_at_cancel_is_reaped_once_as_success() {
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).unwrap();
    let locator = format!(r"\\.\pipe\morrow-m03-complete-{}", crate::wire::hex(&nonce));
    let mut server = Pipe::create_private(&locator).unwrap();
    server.begin_connect().unwrap();
    let mut client = Pipe::open_client(&locator).unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    let wait = |pipe: &mut Pipe, kind| loop {
        if let Some(done) = pipe.poll(kind).unwrap() {
            break done;
        }
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(wait(&mut server, Kind::Connect).error, None);
    client.begin_read(128).unwrap();
    let issued = server.begin_write(vec![0x31; 128]).unwrap();
    assert_eq!(wait(&mut client, Kind::Read).bytes, vec![0x31; 128]);
    server.cancel_all().unwrap();
    let done = wait(&mut server, Kind::Write);
    assert_eq!(done.id, issued.id);
    assert_eq!(done.error, None);
    assert_eq!(done.transferred, 128);
    assert!(!server.has_operation(Kind::Write));
    assert!(server.poll(Kind::Write).is_err());
    assert!(server.cancel_and_reap().unwrap().is_empty());
    assert!(client.cancel_and_reap().unwrap().is_empty());
    println!(
        "completed_at_cancel={}",
        json!({"operation":done.id,"bytes":done.transferred,
        "error":done.error,"reap_count":1,"remaining_operations":0})
    );
}

enum PrefixStop {
    Deadline,
    PeerDisconnect,
}

// Reach the fault through an observed peer prefix and a still-pending real
// operation, rather than assuming a delay means that WriteFile is pending.
fn prefix_fault(stop: PrefixStop) -> Value {
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).unwrap();
    let locator = format!(r"\\.\pipe\morrow-m03-fault-{}", crate::wire::hex(&nonce));
    let gate = Arc::new(Mutex::new(EffectGate {
        revoked: false,
        deadline: Instant::now() + Duration::from_secs(5),
        ordinal: 0,
        last_write: 0,
        issued_body_end: 0,
        network_pending: false,
    }));
    let mut driver = Driver::spawn(locator.clone(), std::process::id(), gate.clone()).unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    let mut client = Some(loop {
        if let Ok(client) = Pipe::open_client(&locator) {
            break client;
        }
        assert!(Instant::now() < until, "fault client connection timed out");
        thread::sleep(Duration::from_millis(1));
    });
    loop {
        if matches!(driver.event(), Some(Event::Connected { .. })) {
            break;
        }
        assert!(Instant::now() < until, "fault driver connection timed out");
        thread::sleep(Duration::from_millis(1));
    }
    client.as_mut().unwrap().begin_read(1024).unwrap();
    driver.send(Command::Write {
        bytes: vec![0x47; 8192],
        body_end: Some(8192),
    }).unwrap();
    let mut observations = Vec::new();
    let mut complete_frames = 0;
    let mut errors = Vec::new();
    let drain = |driver: &Driver, observations: &mut Vec<Value>, complete: &mut usize,
                 errors: &mut Vec<String>| {
        while let Some(event) = driver.event() {
            match event {
                Event::OwnerObservation(v) => observations.push(v),
                Event::WriteCompleted { .. } => *complete += 1,
                Event::Error(e) => errors.push(e),
                _ => {}
            }
        }
    };
    let prefix = loop {
        drain(&driver, &mut observations, &mut complete_frames, &mut errors);
        if let Some(done) = client.as_mut().unwrap().poll(Kind::Read).unwrap() {
            break done;
        }
        assert!(Instant::now() < until, "peer prefix not observed");
        thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(prefix.error, None);
    assert_eq!(prefix.bytes, vec![0x47; 1024]);
    while !observations.iter().any(|o| o["stage"] == "incomplete_sample") {
        drain(&driver, &mut observations, &mut complete_frames, &mut errors);
        assert!(Instant::now() < until, "pending write not observed");
        thread::sleep(Duration::from_millis(1));
    }
    match stop {
        // Only shorten this test lease after the qualifying observation.
        // No explicit Driver::cancel call supplies the result being tested.
        PrefixStop::Deadline => gate.lock().unwrap().deadline = Instant::now(),
        PrefixStop::PeerDisconnect => {
            let peer = client.take().unwrap();
            assert!(!peer.has_operation(Kind::Read));
            drop(peer);
        }
    }
    let closed = loop {
        drain(&driver, &mut observations, &mut complete_frames, &mut errors);
        if let Some(done) = driver.join_if_finished() {
            drain(&driver, &mut observations, &mut complete_frames, &mut errors);
            break done.unwrap();
        }
        assert!(Instant::now() < until, "fault owner join unconfirmed");
        thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(complete_frames, 0);
    let issues: Vec<_> = observations.iter().filter(|o| o["stage"] == "write_issue").collect();
    let reaps: Vec<_> = observations.iter().filter(|o| o["stage"] == "write_reaped").collect();
    assert_eq!(issues.len(), 1);
    assert_eq!(reaps.len(), 1);
    assert_eq!(issues[0]["operation"]["id"], reaps[0]["outcome"]["id"]);
    assert_eq!(issues[0]["operation"]["issue_ordinal"], reaps[0]["operation"]["issue_ordinal"]);
    assert_eq!(issues[0]["operation"]["requested_bytes"], 8192);
    match stop {
        PrefixStop::Deadline => {
            assert!(closed.error.is_none());
            assert!(errors.is_empty());
            assert_eq!(reaps[0]["outcome"]["error"], 995);
            assert!(observations.iter().any(|o| o["stage"] == "cancel_request_observed"
                && o["outcome"]["gate_or_deadline"] == true));
        }
        PrefixStop::PeerDisconnect => {
            assert!(closed.error.is_some());
            assert_eq!(errors.len(), 1);
            // Windows connection failures, not a manufactured success or abort.
            assert!(matches!(reaps[0]["outcome"]["error"].as_u64(), Some(109 | 232 | 233)));
        }
    }
    json!({"peer_observed_bytes":1024,"full_frame_bytes":8192,
        "complete_frames":complete_frames,"owner_joined":true,
        "error":closed.error,"events_errors":errors,"observations":observations})
}

#[test]
fn original_write_pending_at_deadline_is_reaped_without_complete_or_retry() {
    println!("deadline_prefix={}", prefix_fault(PrefixStop::Deadline));
}

#[test]
fn peer_disconnect_after_prefix_retains_error_and_reaps_without_retry() {
    println!("disconnected_prefix={}", prefix_fault(PrefixStop::PeerDisconnect));
}

#[test]
fn real_same_user_wrong_pid_connection_rejected_before_business_io() {
    use std::io::Write;
    use std::process::{Command as ProcessCommand, Stdio};
    let executable = std::env::var("MORROW_ISOLATION_TEST_PEER").expect("real peer required");
    struct ChildGuard(Option<std::process::Child>);
    impl Drop for ChildGuard {
        fn drop(&mut self) { if let Some(child)=self.0.as_mut() { let _=child.kill();let _=child.wait(); } }
    }
    let mut guard = ChildGuard(Some(ProcessCommand::new(executable).stdin(Stdio::piped())
        .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap()));
    let child=guard.0.as_mut().unwrap();
    let actual_pid = child.id();
    assert_ne!(actual_pid, std::process::id());
    let mut nonce = [0;16];getrandom::fill(&mut nonce).unwrap();
    let locator = format!(r"\\.\pipe\morrow-m03-isolation-{}", crate::wire::hex(&nonce));
    let gate = Arc::new(Mutex::new(EffectGate { revoked:false,deadline:Instant::now()+Duration::from_secs(3),
        ordinal:0,last_write:0,issued_body_end:0,network_pending:false }));
    let mut driver=Driver::spawn(locator.clone(),std::process::id(),gate.clone()).unwrap();
    let deadline=Instant::now()+Duration::from_secs(3);
    let mut events=Vec::new();
    loop {
        if let Some(e)=driver.event(){let created=matches!(e,Event::Created{..});events.push(e);if created{break;}}
        assert!(Instant::now()<deadline);thread::sleep(Duration::from_millis(1));
    }
    writeln!(child.stdin.take().unwrap(),"{}",locator).unwrap();
    let result=loop {
        while let Some(e)=driver.event(){events.push(e);}
        if let Some(result)=driver.join_if_finished(){break result.unwrap();}
        assert!(Instant::now()<deadline,"actual owner join missing");thread::sleep(Duration::from_millis(1));
    };
    while let Some(e)=driver.event(){events.push(e);}
    while child.try_wait().unwrap().is_none() {
        assert!(Instant::now()<deadline,"connector process exit missing");thread::sleep(Duration::from_millis(1));
    }
    let output=guard.0.take().unwrap().wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(),actual_pid.to_string());
    assert_eq!(result.error.as_deref(),Some("data peer PID mismatch"));
    assert!(events.iter().any(|e|matches!(e,Event::Error(error) if error=="data peer PID mismatch")));
    assert!(!events.iter().any(|e|matches!(e,Event::Connected{..}|Event::Read(_)|Event::WriteIssued{..}|Event::WriteCompleted{..})));
    assert_eq!(gate.lock().unwrap().ordinal,0);
    println!("same_user_pid_rejection={}",serde_json::json!({"actual_pid":actual_pid,"expected_pid":std::process::id(),"owner_joined":true,"business_io":false,"same_user_open_succeeded":true,"isolation_claimed":false}));
}
