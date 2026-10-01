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
    watch_pids:Vec<u32>,
    controller_timeout_ms:u64,
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
        watch_pids:vec![],
        controller_timeout_ms:2000,
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
            "--watch-pid"=>{let pid:u32=v.parse().map_err(|_|"watch PID")?;if pid==0 || a.watch_pids.contains(&pid) || a.watch_pids.len()>=2{return Err("watch PID list".into());}a.watch_pids.push(pid);},
            "--controller-timeout-ms"=>{a.controller_timeout_ms=v.parse().map_err(|_|"controller timeout")?;if !(100..=5000).contains(&a.controller_timeout_ms){return Err("controller timeout range".into());}},
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
    if let Err(e) = run(false).await {
        eprintln!("native owner host: {e}");
        std::process::exit(2)
    }
}
async fn run(supervised:bool) -> Result<()> {
    let mut a = args()?;
    if !supervised && (!a.watch_pids.is_empty() || a.controller_timeout_ms!=2000) {return Err("supervisor-only options".into());}
    if supervised {morrow_native_pipe_win::job::seal_standard_handles_noninherit().map_err(|e|e.to_string())?;}
    let mut out = tokio::io::stdout();
    if a.mode == "init" {
        if supervised {HostAuthority::initialize_supervised(&a.profile,&a.slot)?;}else{HostAuthority::initialize(&a.profile, &a.slot)?;}
        return emit(&mut out, json!({"event":"initialized","slot":a.slot})).await;
    }
    let mut authority = if supervised {HostAuthority::open_supervised(&a.profile)?}else{HostAuthority::open(&a.profile)?};
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
    let controller=if supervised {Some(morrow_native_session_stream::controller_watch::ControllerWatch::start(&a.watch_pids,Duration::from_millis(a.controller_timeout_ms))?)}else{None};
    let mut controller_loss_observed=false;
    if let Some(controller)=&controller {authority.register_controller(controller.registration())?;}
    let reader_watch=controller.as_ref().map(|c|c.registration());
    let deadline_watch=reader_watch.clone();
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
                    Ok(0) => {if let Some(watch)=&reader_watch{watch.disconnect();}break;},
                    Ok(_) if b.len() <= 1024 => {
                        String::from_utf8(b).map_err(|_| "input UTF8".to_owned())
                    }
                    _ => Err("input limit/read error".into()),
                };
                if let Some(watch)=&reader_watch {
                    if line.is_err(){watch.disconnect();}
                    // A saturated control queue cannot hide EOF or extend a lease.
                    if tx.try_send(line).is_err(){watch.disconnect();break;}
                } else if tx.blocking_send(line).is_err(){break;}

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
    let serving=async {loop {
        tokio::select! {
         line=rx.recv(),if input_open=>{
          let mut action=String::new();
          let result=match line{
           Some(Ok(line))=>match serde_json::from_str::<Value>(&line){Ok(v)=>{
            action=v["action"].as_str().unwrap_or("").to_owned();
            if controller.as_ref().is_some_and(|c|c.lost()) && !matches!(action.as_str(),"inspect"|"inspect_http"|"stop"|"quit") {Err("controller lease lost; no new authority".into())}
            else if !operator_fields_valid(&v){Err("unknown operator field".into())}else{
             let id=v["grant_id"].as_str().unwrap_or("");match action.as_str(){
              "heartbeat" if supervised=>controller.as_ref().unwrap().heartbeat().map(|_|json!({"received":true,"authority_deadline_renewed":false})),
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
           None=>{if let Some(c)=&controller{c.disconnect();}input_open=false;quitting=true;authority.stop().await.map(|_|json!({"input_closed":true}))}
          };
          rejected|=result.is_err();let row=match result{Ok(v)=>json!({"event":"operator_result","action":action,"ok":true,"result":v}),Err(e)=>json!({"event":"operator_result","action":action,"ok":false,"error":e})};
          if output_ok{output_ok=emit(&mut out,row).await.is_ok();}if !output_ok{quitting=true;let _=authority.stop().await;}
         }
         _=interval.tick()=>{
          if controller.as_ref().is_some_and(|c|c.lost()) && !controller_loss_observed {
            controller_loss_observed=true;rejected=true;
            let _=authority.stop().await;
            if output_ok {output_ok=emit(&mut out,json!({"event":"controller_lost","business_gate_closed":true,"authority_deadline_renewed":false})).await.is_ok();}
          }

          let released=match authority.poll().await{Ok(v)=>v,Err(e)=>{rejected=true;let _=authority.stop().await;if output_ok{output_ok=emit(&mut out,json!({"event":"owner_error","error":e})).await.is_ok();}false}};
          for event in &authority.events[cursor..]{if output_ok{output_ok=emit(&mut out,json!({"event":"authority_observation","observation":event})).await.is_ok();}}cursor=authority.events.len();
          if let Some(s)=authority.snapshot(){
           for event in &s.events[session_cursor..]{if output_ok{output_ok=emit(&mut out,json!({"event":"host_observation","pid":s.pid,"session":s.session,"epoch":s.epoch,"observation":event})).await.is_ok();}}session_cursor=s.events.len();
           if supervised && s.phase=="ClosingUnconfirmed" {
            if output_ok{let _=emit(&mut out,json!({"event":"cleanup_unconfirmed","snapshot":s,"owner_retained":true,"business_success_claimed":false})).await;}
            return Err("supervisor cleanup unconfirmed; owner retained".into());
           }
           if released{
            let reason=s.events.iter().find(|e|e["event"]=="session_close_reason").and_then(|e|e["detail"]["reason"].as_u64());
            let expiry_terminal_complete=expired_terminal_confirmed(&s);
            rejected|=!(matches!(reason,Some(19|25)) || (reason==Some(20) && expiry_terminal_complete))||s.events.iter().any(|e|e["event"]=="close_ack_failed");
            if output_ok{output_ok=emit(&mut out,json!({"event":"final","snapshot":s,"state":authority.inspect(None)?,"expired_terminal_protocol_complete":expiry_terminal_complete,"business_success_claimed":false})).await.is_ok();}
            return if rejected||!output_ok{Err("rejected session/operator request; release observed".into())}else{Ok(())}
           }
          }else if quitting{
           let state=authority.inspect(None)?;if output_ok{output_ok=emit(&mut out,json!({"event":"final_without_session","state":state})).await.is_ok();}
           return if rejected||!output_ok{Err("operator request rejected".into())}else{Ok(())}
          }
         }
        }
    }
    };
    if let Some(watch)=deadline_watch {
        tokio::select! {biased;
            _=watch.reclamation_deadline()=>Err("fixed controller-loss cleanup deadline; durable owner retained".into()),
            result=serving=>result,
        }
    } else {serving.await}
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

// Exit 0 for expiry means a proven terminal-control/resource exchange only.
// It never upgrades the expired HTTP business result or durable Unknown.
fn expired_terminal_confirmed(s: &morrow_native_session_stream::Snapshot) -> bool {
    if s.phase!="Released" || s.owner_retained || !s.exit_observed || !s.stdout_eof || !s.stderr_eof || s.event_overflow!=0 { return false; }
    let p=&s.http["progress"];
    for field in ["revoke_persisted","revoke_applied","request_closed","data_closed","connect_reaped","read_reaped","write_reaped","owner_released","child_exited","stdout_eof","stderr_eof"] {
        if p[field]!=true { return false; }
    }
    if p["worker_started"]==true && p["worker_joined"]!=true { return false; }
    if p["error_code"]!=20 { return false; }
    if s.events.iter().any(|e|matches!(e["event"].as_str(),Some("close_ack_failed"|"cancel_persistence_unconfirmed"|"owner_retained"|"kill_requested"|"pipe_driver_error"|"network_supervision_error"|"stderr_limit"|"stderr_error"|"stdout_error"|"wait_unconfirmed"))) { return false; }
    let rows=|name:&str|s.events.iter().filter(|e|e["event"]==name).collect::<Vec<_>>();
    let reasons=rows("session_close_reason");let bounds=rows("expiry_teardown_bound");
    let closes=rows("expiry_teardown_close_received");let acks=rows("expiry_teardown_ack_written");let writes=rows("close_ack_written");
    if reasons.len()!=1 || reasons[0]["detail"]["reason"]!=20 || bounds.len()!=1 || closes.len()!=1 || acks.len()!=1 || writes.len()!=1 { return false; }
    let b=&bounds[0]["detail"];let a=&acks[0]["detail"];
    let Some(deadline)=b["deadline_offset_ns"].as_u64() else{return false};
    let Some(original)=b["original_deadline_offset_ns"].as_u64() else{return false};
    let Some(close_ms)=b["close_ms"].as_u64() else{return false};
    let Some(completed)=a["completed_offset_ns"].as_u64() else{return false};
    let Some(sequence)=closes[0]["detail"]["sequence"].as_u64() else{return false};
    b["authority_deadline_renewed"]==false && b["business_gate_closed"]==true
        && close_ms>0 && deadline>original && completed>=original && completed<deadline
        && a["deadline_offset_ns"].as_u64()==Some(deadline)
        && a["sequence"].as_u64()==Some(sequence) && writes[0]["detail"]["sequence"].as_u64()==Some(sequence)
        && closes[0]["detail"]["first_reason"]==20 && a["business_success"]==false
}

#[cfg(test)]
mod expired_terminal_cli_tests {
    use super::*;
    fn closed() -> morrow_native_session_stream::Snapshot {
        let v=json!({"session":1,"epoch":1,"pid":1,"generation":2,"phase":"Released","owner_retained":false,"exit_code":2,
            "exit_observed":true,"stdout_eof":true,"stderr_eof":true,"event_overflow":0,
            "http":{"progress":{"revoke_persisted":true,"revoke_applied":true,"request_closed":true,"data_closed":true,"connect_reaped":true,"read_reaped":true,"write_reaped":true,"owner_released":true,"child_exited":true,"stdout_eof":true,"stderr_eof":true,"worker_started":true,"worker_joined":true,"error_code":20,"intent":"Unknown"}},
            "events":[{"event":"session_close_reason","detail":{"reason":20}},
                {"event":"expiry_teardown_bound","detail":{"original_deadline_offset_ns":1000000,"deadline_offset_ns":2000000,"close_ms":1,"business_gate_closed":true,"authority_deadline_renewed":false}},
                {"event":"expiry_teardown_close_received","detail":{"sequence":9,"first_reason":20}},
                {"event":"close_ack_written","detail":{"sequence":9}},
                {"event":"expiry_teardown_ack_written","detail":{"sequence":9,"deadline_offset_ns":2000000,"completed_offset_ns":1500000,"business_success":false}}]});
        morrow_native_session_stream::Snapshot { session:1,epoch:1,pid:1,generation:2,phase:"Released".into(),owner_retained:false,exit_code:Some(2),exit_observed:true,stdout_eof:true,stderr_eof:true,event_overflow:0,events:v["events"].as_array().unwrap().clone(),http:v["http"].clone() }
    }
    #[test]
    fn expired_business_unknown_can_have_independent_complete_terminal_protocol() {
        let s=closed();assert!(expired_terminal_confirmed(&s));assert_eq!(s.http["progress"]["intent"],"Unknown");
    }
    #[test]
    fn missing_ack_partial_recovery_late_ack_or_failed_close_remain_rejected() {
        let mut s=closed();s.events.retain(|e|e["event"]!="close_ack_written");assert!(!expired_terminal_confirmed(&s));
        for field in ["request_closed","read_reaped","worker_joined","revoke_persisted"] {let mut s=closed();s.http["progress"][field]=json!(false);assert!(!expired_terminal_confirmed(&s));}
        let mut s=closed();s.events[4]["detail"]["completed_offset_ns"]=json!(2000000);assert!(!expired_terminal_confirmed(&s));
        let mut s=closed();s.events.push(json!({"event":"close_ack_failed"}));assert!(!expired_terminal_confirmed(&s));
        let mut s=closed();s.events[3]["detail"]["sequence"]=json!(10);assert!(!expired_terminal_confirmed(&s));
    }
}
