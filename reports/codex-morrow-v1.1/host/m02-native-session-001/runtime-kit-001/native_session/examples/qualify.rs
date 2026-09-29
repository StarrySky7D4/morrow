//! Real child/pipe harness. Temporary host approval is explicit here, not in client wire.
use morrow_native_session::{Admission, LaunchSpec, NativeHost, Session, Snapshot, wire};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
async fn until(s: &Session, pred: impl Fn(&Snapshot) -> bool) -> Snapshot {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let st = s.snapshot();
            if pred(&st) {
                return st;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "timeout snapshot {}",
            serde_json::to_string(&s.snapshot()).unwrap()
        )
    })
}
fn spec(exe: &PathBuf, root: &PathBuf, mode: &str) -> LaunchSpec {
    let dir = root.join(mode);
    std::fs::create_dir(&dir).unwrap();
    LaunchSpec {
        slot: "one-owner".into(),
        executable: exe.clone(),
        artifact_sha256: wire::digest(&std::fs::read(exe).unwrap()),
        cwd: dir,
        args: vec![mode.into()],
        ttl_ms: 2500,
        handshake_ms: 700,
        frame_ms: 200,
        close_ms: 100,
        request_budget: 64,
    }
}
fn event(st: &Snapshot, name: &str) -> bool {
    st.events.iter().any(|e| e["event"] == name)
}
fn reason(st: &Snapshot, code: u64) -> bool {
    st.events
        .iter()
        .any(|e| e["event"] == "stop_requested" && e["detail"]["reason"] == code)
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let mut args = std::env::args_os().skip(1);
    let exe = PathBuf::from(args.next().expect("fixture exe"));
    let root = PathBuf::from(args.next().expect("fresh output directory"));
    std::fs::create_dir(&root).unwrap();
    let mut cases: Vec<Value> = vec![];
    for (mode, expected) in [
        ("normal", 25),
        ("wrong-epoch", 17),
        ("wrong-hash", 17),
        ("nonzero-code", 17),
        ("malformed", 16),
        ("duplicate-hello", 18),
        ("oversize", 16),
        ("partial", 16),
        ("silent", 23),
        ("disconnect", 24),
    ] {
        let mut host = NativeHost::new().unwrap();
        let s = host
            .launch(Admission::authorize(spec(&exe, &root, mode)).unwrap())
            .unwrap();
        let st = until(&s, |s| s.phase == "Released").await;
        assert!(reason(&st, expected), "{mode}: {st:?}");
        assert!(st.exit_observed && st.stdout_eof && st.stderr_eof && !st.owner_retained);
        assert_eq!(st.event_overflow, 0);
        cases.push(json!({"case":mode,"snapshot":st}));
    }
    {
        let mut host = NativeHost::new().unwrap();
        let mut sp = spec(&exe, &root, "ignore-stop");
        sp.ttl_ms = 200;
        let s = host.launch(Admission::authorize(sp).unwrap()).unwrap();
        let st = until(&s, |s| s.phase == "Released").await;
        assert!(reason(&st, 20));
        assert!(event(&st, "kill_requested"));
        cases.push(json!({"case":"runtime_expiry_forced_exit","snapshot":st}));
    }
    {
        let mut host = NativeHost::new().unwrap();
        let mut sp = spec(&exe, &root, "tick");
        sp.ttl_ms = 700;
        let s = host.launch(Admission::authorize(sp).unwrap()).unwrap();
        until(&s, |s| event(s, "state_read")).await;
        s.revoke().await.unwrap();
        let rev = until(&s, |s| {
            s.events
                .iter()
                .any(|e| e["event"] == "request_denied" && e["detail"]["code"] == 19)
        })
        .await;
        let ack = rev
            .events
            .iter()
            .position(|e| e["event"] == "control_ack")
            .unwrap();
        assert!(
            !rev.events[ack + 1..]
                .iter()
                .any(|e| e["event"] == "state_read")
        );
        assert_eq!(rev.generation, 2);
        s.revoke().await.unwrap();
        assert_eq!(s.snapshot().generation, 2);
        let st = until(&s, |s| s.phase == "Released").await;
        assert!(reason(&st,20));
        cases.push(json!({"case":"online_revoke_and_retry_no_renewal","snapshot":st}));
    }
    {
        let mut host = NativeHost::new().unwrap();
        let mut sp = spec(&exe, &root, "no-read");
        sp.frame_ms = 1000;
        let s = host.launch(Admission::authorize(sp).unwrap()).unwrap();
        until(&s, |s| s.phase == "Active").await;
        let begin = std::time::Instant::now();
        s.revoke().await.unwrap();
        s.stop().await.unwrap();
        let elapsed = begin.elapsed().as_millis();
        assert!(elapsed < 500);
        let st = until(&s, |s| s.phase == "Released").await;
        cases.push(
            json!({"case":"control_with_nonreading_peer","control_ack_ms":elapsed,"snapshot":st}),
        );
    }
    {
        let mut host = NativeHost::new().unwrap();
        let sp = spec(&exe, &root, "hold-output");
        let s = host
            .launch(Admission::authorize(sp.clone()).unwrap())
            .unwrap();
        let retained = until(&s, |s| s.phase == "ClosingUnconfirmed").await;
        assert!(retained.exit_observed && retained.owner_retained);
        assert!(!retained.stdout_eof || !retained.stderr_eof);
        assert!(
            host.launch(Admission::authorize(sp).unwrap())
                .err()
                .unwrap()
                .contains("owner retained")
        );
        let st = until(&s, |s| s.phase == "Released").await;
        cases.push(json!({"case":"exit_before_output_close_retains_owner","unconfirmed":retained,"snapshot":st}));
    }
    {
        let mut host=NativeHost::new().unwrap();
        let mut first=spec(&exe,&root,"epoch-first");first.args=vec!["normal".into()];
        let a=host.launch(Admission::authorize(first).unwrap()).unwrap();until(&a,|s|s.phase=="Released").await;
        let next=host.launch(Admission::authorize(spec(&exe,&root,"stale-epoch")).unwrap()).unwrap();
        let st=until(&next,|s|s.phase=="Released").await;assert_eq!(st.epoch,2);assert!(reason(&st,17));
        cases.push(json!({"case":"same_slot_new_epoch_rejects_old_epoch","snapshot":st}));
    }
    {
        let mut host=NativeHost::new().unwrap();let mut sp=spec(&exe,&root,"flood");sp.request_budget=2;
        let s=host.launch(Admission::authorize(sp).unwrap()).unwrap();let st=until(&s,|s|s.phase=="Released").await;
        assert!(reason(&st,21));assert_eq!(st.events.iter().filter(|e|e["event"]=="state_read").count(),2);
        cases.push(json!({"case":"cumulative_request_quota","snapshot":st}));
    }
    {
        let mut host = NativeHost::new().unwrap();
        let mut sp = spec(&exe, &root, "expired-before-launch");
        sp.ttl_ms = 100;
        let admission = Admission::authorize(sp).unwrap();
        tokio::time::sleep(Duration::from_millis(120)).await;
        let error = host.launch(admission).err().unwrap();
        assert!(error.contains("expired before spawn"));
        cases.push(json!({"case":"first_admission_deadline","error":error}));
    }
    {
        let mut sp = spec(&exe, &root, "wrong-artifact-approval");
        sp.artifact_sha256[0] ^= 1;
        let error = Admission::authorize(sp).err().unwrap();
        assert!(error.contains("digest mismatch"));
        cases.push(json!({"case":"trusted_artifact_mismatch","error":error}));
    }
    let result = json!({"status":"passed_real_local_control_slice","cases":cases,"case_count":cases.len(),"not_claimed":["production approval UI","trusted install/image race closure","OS isolation","business success","M02 complete"]});
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("result.json"))
        .unwrap();
    serde_json::to_writer_pretty(file, &result).unwrap();
    println!(
        "{}",
        json!({"status":result["status"],"case_count":result["case_count"]})
    );
}
