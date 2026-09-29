//! Real local pipe operations only: no child, protocol guest, Core or HTTP.
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
