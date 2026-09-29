use morrow_native_session_owner::{LaunchSpec, Result, authority::HostAuthority, wire};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use tokio::{io::AsyncWriteExt, sync::mpsc};
struct Args {
    mode: String,
    profile: PathBuf,
    slot: String,
    spec: LaunchSpec,
    plugin: String,
    role: String,
    operation: String,
}
fn args() -> Result<Args> {
    let mut it = std::env::args().skip(1);
    let mode = it.next().ok_or("expected init, inspect or serve")?;
    let mut a = Args {
        mode,
        profile: PathBuf::new(),
        slot: String::new(),
        plugin: String::new(),
        role: String::new(),
        operation: String::new(),
        spec: LaunchSpec {
            slot: String::new(),
            executable: PathBuf::new(),
            artifact_sha256: [0; 32],
            cwd: PathBuf::new(),
            args: vec![],
            ttl_ms: 10000,
            handshake_ms: 2500,
            frame_ms: 500,
            close_ms: 300,
            request_budget: 64,
        },
    };
    while let Some(key) = it.next() {
        let v = it.next().ok_or("missing argument value")?;
        match key.as_str() {
            "--profile" => a.profile = v.into(),
            "--slot" => a.slot = v,
            "--client" => a.spec.executable = v.into(),
            "--work-dir" => a.spec.cwd = v.into(),
            "--client-arg" => a.spec.args.push(v),
            "--plugin-id" => a.plugin = v,
            "--role" => a.role = v,
            "--operation" => a.operation = v,
            "--sha256" => {
                if v.len() != 64 || !v.is_ascii() {
                    return Err("digest length/encoding".into());
                }
                for i in 0..32 {
                    a.spec.artifact_sha256[i] =
                        u8::from_str_radix(&v[i * 2..i * 2 + 2], 16).map_err(|_| "digest hex")?
                }
            }
            "--ttl-ms" => a.spec.ttl_ms = v.parse().map_err(|_| "ttl")?,
            "--handshake-ms" => a.spec.handshake_ms = v.parse().map_err(|_| "handshake")?,
            "--frame-ms" => a.spec.frame_ms = v.parse().map_err(|_| "frame")?,
            "--close-ms" => a.spec.close_ms = v.parse().map_err(|_| "close")?,
            "--budget" => a.spec.request_budget = v.parse().map_err(|_| "budget")?,
            _ => return Err("unknown option".into()),
        }
    }
    Ok(a)
}
async fn emit(out: &mut tokio::io::Stdout, v: Value) -> Result<()> {
    let mut bytes = serde_json::to_vec(&v).unwrap();
    bytes.push(b'\n');
    tokio::time::timeout(Duration::from_secs(1), async {
        out.write_all(&bytes).await?;
        out.flush().await
    })
    .await
    .map_err(|_| "output timeout")?
    .map_err(|e| e.to_string())
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("native owner host: {e}");
        std::process::exit(2)
    }
}
async fn run() -> Result<()> {
    let mut a = args()?;
    let mut out = tokio::io::stdout();
    if a.mode == "init" {
        HostAuthority::initialize(&a.profile, &a.slot)?;
        return emit(&mut out, json!({"event":"initialized","slot":a.slot})).await;
    }
    let mut authority = HostAuthority::open(&a.profile)?;
    if a.mode == "inspect" {
        return emit(
            &mut out,
            json!({"event":"inspection","state":authority.inspect(None)?}),
        )
        .await;
    }
    if a.mode != "serve" {
        return Err("unknown mode".into());
    }
    a.spec.slot = authority.slot().into();
    emit(&mut out,json!({"event":"proposal","host_pid":std::process::id(),"approval_required":true,"slot":a.spec.slot,"artifact_sha256":wire::hex(&a.spec.artifact_sha256),"plugin_id":a.plugin,"role":a.role,"operation":a.operation,"production_cli_authenticated":false})).await?;
    let (tx, mut rx) = mpsc::channel(8);
    std::thread::Builder::new()
        .name("trusted-operator-reader".into())
        .spawn(move || {
            use std::io::{BufRead, Read};
            let mut reader = std::io::BufReader::new(std::io::stdin());
            loop {
                let mut b = vec![];
                let line = match reader.by_ref().take(1025).read_until(b'\n', &mut b) {
                    Ok(0) => break,
                    Ok(_) if b.len() <= 1024 => {
                        String::from_utf8(b).map_err(|_| "input UTF8".to_owned())
                    }
                    _ => Err("input limit/read error".into()),
                };
                if tx.blocking_send(line).is_err() {
                    break;
                }
            }
        })
        .map_err(|e| e.to_string())?;
    let mut interval = tokio::time::interval(Duration::from_millis(10));
    let mut cursor = 0;
    let mut session_cursor = 0;
    let mut input_open = true;
    let mut rejected = false;
    let mut quitting = false;
    let mut output_ok = true;
    loop {
        tokio::select! {biased;
         line=rx.recv(),if input_open=>{
          let mut action=String::new();
          let result=match line{
           Some(Ok(line))=>match serde_json::from_str::<Value>(&line){Ok(v)=>{
            action=v["action"].as_str().unwrap_or("").to_owned();
            if !v.as_object().is_some_and(|m|m.keys().all(|k|["action","grant_id"].contains(&k.as_str()))){Err("unknown operator field".into())}else{
             let id=v["grant_id"].as_str().unwrap_or("");match action.as_str(){
              "approve"=>authority.approve(a.spec.clone(),&a.plugin,&a.role,&a.operation).map(|id|json!({"grant_id":id})),
              "claim"=>authority.claim(id).map(|s|json!({"pid":s.pid,"epoch":s.epoch,"session":s.session})),
              "revoke"=>authority.revoke(id).await,
              "inspect"=>authority.inspect(if id.is_empty(){None}else{Some(id)}),
              "stop"=>authority.stop().await.map(|_|json!({"requested":true})),
              "quit"=>{quitting=true;authority.stop().await.map(|_|json!({"requested":true}))},
              _=>Err("unknown trusted action".into())
             }
            }
           },Err(e)=>Err(e.to_string())},
           Some(Err(e))=>Err(e),
           None=>{input_open=false;quitting=true;authority.stop().await.map(|_|json!({"input_closed":true}))}
          };
          rejected|=result.is_err();let row=match result{Ok(v)=>json!({"event":"operator_result","action":action,"ok":true,"result":v}),Err(e)=>json!({"event":"operator_result","action":action,"ok":false,"error":e})};
          if output_ok{output_ok=emit(&mut out,row).await.is_ok();}if !output_ok{quitting=true;let _=authority.stop().await;}
         }
         _=interval.tick()=>{
          let released=match authority.poll().await{Ok(v)=>v,Err(e)=>{rejected=true;let _=authority.stop().await;if output_ok{output_ok=emit(&mut out,json!({"event":"owner_error","error":e})).await.is_ok();}false}};
          for event in &authority.events[cursor..]{if output_ok{output_ok=emit(&mut out,json!({"event":"authority_observation","observation":event})).await.is_ok();}}cursor=authority.events.len();
          if let Some(s)=authority.snapshot(){
           for event in &s.events[session_cursor..]{if output_ok{output_ok=emit(&mut out,json!({"event":"host_observation","pid":s.pid,"session":s.session,"epoch":s.epoch,"observation":event})).await.is_ok();}}session_cursor=s.events.len();
           if released{
            let reason=s.events.iter().find(|e|e["event"]=="revoked").and_then(|e|e["detail"]["reason"].as_u64());rejected|=!matches!(reason,Some(19|25));
            if output_ok{output_ok=emit(&mut out,json!({"event":"final","snapshot":s,"state":authority.inspect(None)?})).await.is_ok();}
            return if rejected||!output_ok{Err("rejected session/operator request; release observed".into())}else{Ok(())}
           }
          }else if quitting{
           let state=authority.inspect(None)?;if output_ok{output_ok=emit(&mut out,json!({"event":"final_without_session","state":state})).await.is_ok();}
           return if rejected||!output_ok{Err("operator request rejected".into())}else{Ok(())}
          }
         }
        }
    }
}
