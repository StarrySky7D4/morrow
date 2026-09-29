//! Trusted harness/operator CLI. No approval is read from the spawned client.
use morrow_native_session::{Admission, LaunchSpec, NativeHost};
use serde_json::json;
use std::{path::PathBuf, time::Duration};
use tokio::io::AsyncWriteExt;
fn args() -> Result<LaunchSpec, String> {
    let mut spec = LaunchSpec {
        slot: "native-control".into(),
        executable: PathBuf::new(),
        artifact_sha256: [0; 32],
        cwd: PathBuf::new(),
        args: vec![],
        ttl_ms: 10000,
        handshake_ms: 2000,
        frame_ms: 500,
        close_ms: 300,
        request_budget: 64,
    };
    let mut a = std::env::args().skip(1);
    while let Some(key) = a.next() {
        if key == "--help" {
            println!(
                "Trusted host harness: --client ABS --sha256 HEX --work-dir EMPTY_TEMP [--ttl-ms N --handshake-ms N --frame-ms N --close-ms N --budget N --client-arg ARG]. stdin JSON: {{\"action\":\"inspect|revoke|stop\"}}. stdout JSON evidence. Control-only; no product approval UI."
            );
            std::process::exit(0);
        }
        let value = a.next().ok_or("missing value")?;
        match key.as_str() {
            "--client" => spec.executable = value.into(),
            "--work-dir" => spec.cwd = value.into(),
            "--client-arg" => spec.args.push(value),
            "--sha256" => {
                if value.len() != 64 {
                    return Err("sha256 length".into());
                }
                for i in 0..32 {
                    spec.artifact_sha256[i] = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)
                        .map_err(|_| "sha256 hex")?;
                }
            }
            "--ttl-ms" => spec.ttl_ms = value.parse().map_err(|_| "ttl")?,
            "--handshake-ms" => spec.handshake_ms = value.parse().map_err(|_| "handshake")?,
            "--frame-ms" => spec.frame_ms = value.parse().map_err(|_| "frame")?,
            "--close-ms" => spec.close_ms = value.parse().map_err(|_| "close")?,
            "--budget" => spec.request_budget = value.parse().map_err(|_| "budget")?,
            _ => return Err("unknown argument".into()),
        }
    }
    Ok(spec)
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("native host failed: {e}");
        std::process::exit(2);
    }
}
async fn run() -> Result<(), String> {
    let mut host = NativeHost::new()?;
    let session = host.launch(Admission::authorize(args()?)?)?;
    // A dedicated OS reader is not a Tokio blocking task: leaving the control pipe
    // open cannot keep the runtime shutdown waiting for an uninterruptible stdin read.
    let (tx, mut input) = tokio::sync::mpsc::channel(8);
    std::thread::Builder::new()
        .name("trusted-control-reader".into())
        .spawn(move || {
            use std::io::{BufRead, Read};
            let mut reader = std::io::BufReader::new(std::io::stdin());
            loop {
                let mut bytes = Vec::new();
                let result = reader.by_ref().take(1025).read_until(b'\n', &mut bytes);
                let line = match result {
                    Ok(0) => break,
                    Ok(_) if bytes.len() <= 1024 => {
                        String::from_utf8(bytes).map_err(|_| "control UTF8".to_string())
                    }
                    _ => Err("control input length/read error".into()),
                };
                if tx.blocking_send(line).is_err() {
                    break;
                }
            }
        })
        .map_err(|e| e.to_string())?;
    let mut stdout = tokio::io::stdout();
    let mut open = true;
    let mut output_ok = true;
    let mut cursor = 0;
    let mut interval = tokio::time::interval(Duration::from_millis(10));
    loop {
        tokio::select! {
            biased;
            line=input.recv(),if open=>{
                let response=match line {
                    Some(Ok(line))=>match serde_json::from_str::<serde_json::Value>(&line) {
                        Ok(value)=>match value["action"].as_str(){Some("revoke")=>session.revoke().await,Some("stop")=>session.stop().await,Some("inspect")=>Ok(()),_=>Err("unknown trusted action".into())},
                        Err(e)=>Err(e.to_string()),
                    },
                    _=>{open=false;session.stop().await}
                };
                if output_ok {output_ok=emit(&mut stdout,&json!({"event":"trusted_control_result","ok":response.is_ok(),"error":response.err(),"snapshot":session.snapshot()})).await;}
                if !output_ok {let _=session.stop().await;}
            }
            _=interval.tick()=>{
                let snapshot=session.snapshot();
                for event in &snapshot.events[cursor..]{
                    if output_ok {output_ok=emit(&mut stdout,&json!({"event":"host_observation","pid":snapshot.pid,"session":snapshot.session,"epoch":snapshot.epoch,"observation":event})).await;}
                }
                cursor=snapshot.events.len();
                if !output_ok && snapshot.owner_retained {let _=session.stop().await;}
                if snapshot.phase=="Released"{
                    let reason=snapshot.events.iter().find(|e|e["event"]=="revoked").and_then(|e|e["detail"]["reason"].as_u64());
                    let accepted_close=matches!(reason,Some(19|25));
                    if output_ok {output_ok=emit(&mut stdout,&json!({"event":"final","session_outcome":if accepted_close{"controlled_close"}else{"rejected_or_disconnected"},"snapshot":snapshot})).await;}
                    return if !output_ok {Err("operator output failed; child release confirmed".into())}else if !accepted_close {Err("session rejected or disconnected; child release confirmed".into())}else{Ok(())};
                }
                // ClosingUnconfirmed deliberately keeps this owner/supervisor alive.
            }
        }
    }
}
async fn emit(stdout: &mut tokio::io::Stdout, value: &serde_json::Value) -> bool {
    let mut bytes = serde_json::to_vec(value).unwrap();
    bytes.push(b'\n');
    matches!(
        tokio::time::timeout(Duration::from_secs(1), async {
            stdout.write_all(&bytes).await?;
            stdout.flush().await
        })
        .await,
        Ok(Ok(()))
    )
}
