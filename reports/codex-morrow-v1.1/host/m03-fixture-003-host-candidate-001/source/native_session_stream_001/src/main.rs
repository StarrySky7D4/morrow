use morrow_native_session_stream::{LaunchSpec, Result, authority::HostAuthority, wire};
#[cfg(feature = "qualification-pipe-fault")]
use morrow_native_session_stream::{PartialFrameWitness, PipeFaultPlan};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use tokio::{io::AsyncWriteExt, sync::mpsc};
struct Args {
    #[cfg(feature = "qualification-pipe-fault")]
    pipe_fault: Option<PipeFaultPlan>,
    mode: String,
    profile: PathBuf,
    slot: String,
    spec: LaunchSpec,
    plugin: String,
    role: String,
    operation: String,
}
fn args() -> Result<Args> {
    parse_args(std::env::args().skip(1))
}
fn parse_args(mut it: impl Iterator<Item = String>) -> Result<Args> {
    let mode = it.next().ok_or("expected init, inspect or serve")?;
    let mut a = Args {
        #[cfg(feature = "qualification-pipe-fault")]
        pipe_fault: None,
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
            request_budget: 128,
            http_origin: String::new(),
        },
    };
    while let Some(key) = it.next() {
        let v = it.next().ok_or("missing argument value")?;
        match key.as_str() {
            #[cfg(feature = "qualification-pipe-fault")]
            "--qualification-pipe-plan" => {
                if a.pipe_fault.is_some() || v.len() > 4096 {
                    return Err("duplicate/oversized qualification plan".into());
                }
                a.pipe_fault =
                    Some(serde_json::from_str(&v).map_err(|_| "qualification plan JSON")?);
            }
            "--profile" => a.profile = v.into(),
            "--slot" => a.slot = v,
            "--client" => a.spec.executable = v.into(),
            "--work-dir" => a.spec.cwd = v.into(),
            "--client-arg" => a.spec.args.push(v),
            "--plugin-id" => a.plugin = v,
            "--role" => a.role = v,
            "--operation" => a.operation = v,
            "--http-origin" => a.spec.http_origin = v,
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
    #[cfg(feature = "qualification-pipe-fault")]
    if a.pipe_fault.is_some() && a.mode != "serve" {
        return Err("qualification pipe plan requires serve mode".into());
    }
    Ok(a)
}
fn operator_fields_valid(v: &Value) -> bool {
    #[cfg(feature = "qualification-pipe-fault")]
    if v["action"] == "qualification_close_data" {
        return v.as_object().is_some_and(|m| {
            m.len() == 3
                && m.keys()
                    .all(|k| ["action", "grant_id", "witness"].contains(&k.as_str()))
        });
    }
    v.as_object().is_some_and(|m| {
        m.keys().all(|k| {
            [
                "action",
                "grant_id",
                "proposal_ref",
                "expected_hash",
                "response_limit",
            ]
            .contains(&k.as_str())
        })
    })
}
fn approve_cli(authority: &mut HostAuthority, a: &Args) -> Result<String> {
    #[cfg(feature = "qualification-pipe-fault")]
    if let Some(plan) = &a.pipe_fault {
        return authority.approve_with_pipe_fault(
            a.spec.clone(),
            &a.plugin,
            &a.role,
            &a.operation,
            plan.clone(),
        );
    }
    authority.approve(a.spec.clone(), &a.plugin, &a.role, &a.operation)
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
            if !operator_fields_valid(&v){Err("unknown operator field".into())}else{
             let id=v["grant_id"].as_str().unwrap_or("");match action.as_str(){
              "approve"=>approve_cli(&mut authority,&a).map(|id|json!({"grant_id":id})),
              #[cfg(feature = "qualification-pipe-fault")]
              "qualification_close_data"=>match serde_json::from_value::<PartialFrameWitness>(v["witness"].clone()){Ok(w)=>authority.qualification_close_data(id,w).await,Err(_)=>Err("qualification witness JSON".into())},
              "claim"=>authority.claim(id).map(|s|json!({"pid":s.pid,"epoch":s.epoch,"session":s.session})),
              "revoke"=>authority.revoke(id).await,
              "inspect_http"=>authority.inspect_http(),
              "approve_http"=>match (hex32(v["proposal_ref"].as_str().unwrap_or("")),hex32(v["expected_hash"].as_str().unwrap_or("")),v["response_limit"].as_u64().and_then(|n|u32::try_from(n).ok())){(Ok(reference),Ok(hash),Some(limit))=>authority.approve_http(reference,hash,limit).await,_=>Err("HTTP approval fields".into())},
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
            let reason=s.events.iter().find(|e|e["event"]=="session_close_reason").and_then(|e|e["detail"]["reason"].as_u64());rejected|=!matches!(reason,Some(19|25))||s.events.iter().any(|e|e["event"]=="close_ack_failed");
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

fn hex32(s: &str) -> Result<Vec<u8>> {
    if s.len() != 64 || !s.is_ascii() {
        return Err("digest format".into());
    }
    (0..32)
        .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|_| "digest hex".into()))
        .collect()
}

#[cfg(test)]
mod qualification_cli_tests {
    use super::*;
    fn p() -> Value {
        json!({"version":1,"scenario":"pipe-partial-close","nonce":"ab".repeat(32),
        "fixture_spec_sha256":"cd".repeat(32),"target":"first-response-body-chunk","prefix_bytes":12,
        "close_trigger":"matched-passive-partial-frame-witness"})
    }
    #[cfg(not(feature = "qualification-pipe-fault"))]
    #[test]
    fn production_cli_rejects_qualification_flag_and_witness_fields() {
        let args = vec![
            "serve".into(),
            "--qualification-pipe-plan".into(),
            p().to_string(),
        ];
        assert!(matches!(parse_args(args.into_iter()),Err(e) if e=="unknown option"));
        assert!(!operator_fields_valid(
            &json!({"action":"qualification_close_data","grant_id":"ab".repeat(32),"witness":{}})
        ));
    }
    #[cfg(feature = "qualification-pipe-fault")]
    #[test]
    fn qualification_cli_requires_explicit_serve_and_strict_plan_json() {
        let args = vec![
            "serve".into(),
            "--qualification-pipe-plan".into(),
            p().to_string(),
        ];
        assert!(
            parse_args(args.clone().into_iter())
                .unwrap()
                .pipe_fault
                .is_some()
        );
        let mut duplicate = args.clone();
        duplicate.extend(["--qualification-pipe-plan".into(), p().to_string()]);
        assert!(parse_args(duplicate.into_iter()).is_err());
        let mut init = args.clone();
        init[0] = "init".into();
        assert!(parse_args(init.into_iter()).is_err());
        let mut extra = p();
        extra["renew_ttl"] = json!(true);
        assert!(
            parse_args(
                vec![
                    "serve".into(),
                    "--qualification-pipe-plan".into(),
                    extra.to_string()
                ]
                .into_iter()
            )
            .is_err()
        );
    }
    #[cfg(feature = "qualification-pipe-fault")]
    #[test]
    fn qualification_operator_witness_is_compact_and_rejects_identity_substitution_fields() {
        let witness = json!({"identity_sha256":"11".repeat(32),"nonce":"ab".repeat(32),"fixture_spec_sha256":"cd".repeat(32),
            "marker_sha256":"ef".repeat(32),"marker_ordinal":1,"read_id":2,"read_issue_count":2,"buffered_bytes":12,
            "declared_payload_bytes":1360,"expected_frame_bytes":1364,"prefix_sha256":"22".repeat(32)});
        let mut action = json!({"action":"qualification_close_data","grant_id":"33".repeat(32),"witness":witness});
        assert!(operator_fields_valid(&action));
        assert!(action.to_string().len() + 1 <= 1024);
        assert!(serde_json::from_value::<PartialFrameWitness>(action["witness"].clone()).is_ok());
        action["witness"]["child_pid"] = json!(1);
        assert!(serde_json::from_value::<PartialFrameWitness>(action["witness"].clone()).is_err());
        action["deadline_ms"] = json!(5000);
        assert!(!operator_fields_valid(&action));
    }
}
