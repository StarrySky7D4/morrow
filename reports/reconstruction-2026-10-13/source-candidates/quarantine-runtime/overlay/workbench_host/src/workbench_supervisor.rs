use morrow_native_pipe_win::{
    controller_lease::{ControllerLease, LeaseSignal},
    job::{Job, prepare_trusted_host_environment, seal_standard_handles_noninherit},
};
#[cfg(feature = "c28-product-session-qualification")]
use morrow_native_pipe_win::job::prepare_qualified_host_environment;
use crate::{
    Result,
    workbench_supervision::{Owner, root_for},
};
use std::{
    collections::BTreeMap,
    io::Read,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc,
};
#[cfg(feature="c28-product-session-qualification")]
#[path="qualification_session_bootstrap.rs"]
mod qualification_session_bootstrap;
const MAX_FRAME: usize = 128 * 1024;
async fn emit_frame(output: &mut tokio::io::Stdout, kind: u8, bytes: &[u8]) -> std::io::Result<()> {
    output
        .write_all(&((bytes.len() + 1) as u32).to_le_bytes())
        .await?;
    output.write_all(&[kind]).await?;
    output.write_all(bytes).await?;
    output.flush().await
}
/// Dead UI output is a controller loss, not a reason to skip trusted cleanup.
/// Once lost, terminal diagnostics are best-effort and bounded; business payload
/// output is discarded. The independent loss ceiling still applies.
async fn emit(output:&mut tokio::io::Stdout,signal:&LeaseSignal,kind:u8,bytes:&[u8])->std::io::Result<()> {
    if signal.lost() {
        if kind!=1 {let _=tokio::time::timeout(Duration::from_millis(25),emit_frame(output,kind,bytes)).await;}
        return Ok(());
    }
    if emit_frame(output,kind,bytes).await.is_err() {signal.disconnect();}
    Ok(())
}
enum Command {
    Data(zeroize::Zeroizing<Vec<u8>>),
    Close,
    QuarantineClose,
}
// This terminal is not a normal Close, business ACK, release or replay permit.
#[derive(PartialEq, Eq)]
enum ControllerReaderExit { Lost, QuarantineClose }
fn quarantine_control_allowed(frame: &[u8], abnormal: bool) -> bool {
    abnormal && frame == [4]
}
pub async fn run() -> Result<u32> {
    if let Some(code) = management().await? { return Ok(code); }
    let mut args = std::env::args().skip(1);
    let ui: u32 = args.next().ok_or("UI PID")?.parse()?;
    let host = PathBuf::from(args.next().ok_or("host path")?);
    let mut database = args.next().ok_or("database path")?;
    let managed = database == "--managed";
    if managed {
        database = args.next().ok_or("library root")?;
    }
    let package = PathBuf::from(args.next().ok_or("package path")?);
    #[cfg(feature="c28-product-session-qualification")]
    let mut qualification = qualification_session_bootstrap::Admission::parse(
        &mut args, &database, &package, &host, Some(&std::env::current_exe()?), managed)?;
    #[cfg(not(feature="c28-product-session-qualification"))]
    if args.next().is_some() {
        return Err("unexpected supervisor arguments".into());
    }
    let host = host.canonicalize()?;
    let package = package.canonicalize()?;
    let mut environment = BTreeMap::new();
    for name in ["SystemRoot", "WINDIR", "COMSPEC"] {
        if let Ok(v) = std::env::var(name) {
            environment.insert(name.into(), v);
        }
    }
    // Resolve/encode once before profile, root, seal, lease, reader, Owner or Job effects.
    // Resolver failure here precedes those effects; namespace qualification remains OPEN.
    #[cfg(feature = "c28-product-session-qualification")]
    let prepared_environment = match &qualification {
        Some(admission) => prepare_qualified_host_environment(&environment, admission.plan.digest)?,
        None => prepare_trusted_host_environment(&environment)?,
    };
    #[cfg(not(feature = "c28-product-session-qualification"))]
    let prepared_environment = prepare_trusted_host_environment(&environment)?;
    #[cfg(feature="c28-product-session-qualification")]
    if let Some(admission) = &mut qualification { admission.create_profile_once()?; }
    let root = root_for(std::path::Path::new(&database), managed)?;
    seal_standard_handles_noninherit()?;
    let signal = LeaseSignal::default();
    let startup = Arc::new(Mutex::new(()));
    let starting = startup.clone();
    let revocation = Arc::new(Mutex::new(
        None::<morrow_native_pipe_win::job::RevocationEvent>,
    ));
    let revoked = revocation.clone();
    let gate = Arc::new(AtomicBool::new(false));
    let stopped = gate.clone();
    let _lease = ControllerLease::start(
        signal.clone(),
        &[ui],
        Duration::from_secs(5),
        Arc::new(move || {
            let _start = starting.lock().unwrap_or_else(|e| e.into_inner());
            stopped.store(true, Ordering::Release);
            if let Some(event) = revoked.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                if event.revoke().is_err() {
                    morrow_native_pipe_win::job::terminate_current_process(2);
                }
            }
        }),
    )?;
    let (send, mut commands) = mpsc::channel(8);
    let reader_signal = signal.clone();
    let reader_gate = gate.clone();
    let graceful = Arc::new(AtomicBool::new(false));
    let reading_graceful = graceful.clone();
    let abnormal_closing = Arc::new(AtomicBool::new(false));
    let reader_abnormal = abnormal_closing.clone();
    let mut controller_reader = Some(std::thread::Builder::new()
        .name("workbench-supervisor-controller".into())
        .spawn(move || {
            let mut input = std::io::stdin().lock();
            loop {
                let mut header = [0; 4];
                if input.read_exact(&mut header).is_err() {
                    reader_signal.disconnect();
                    reader_gate.store(true, Ordering::Release);
                    break;
                }
                let size = u32::from_le_bytes(header) as usize;
                if size == 0 || size > MAX_FRAME + 1 {
                    reader_signal.disconnect();
                    break;
                }
                let mut frame = zeroize::Zeroizing::new(vec![0; size]);
                if input.read_exact(&mut frame).is_err() {
                    reader_signal.disconnect();
                    break;
                }
                let command = match frame[0] {
                    4 if quarantine_control_allowed(&frame, reader_abnormal.load(Ordering::Acquire)) => {
                        Command::QuarantineClose
                    }
                    2 if size == 1 => {
                        // A terminal quarantine request may follow the last in-flight
                        // heartbeat. Do not renew or clear the original loss clock.
                        if reader_abnormal.load(Ordering::Acquire) { continue; }
                        if reader_signal.heartbeat().is_err() {
                            break;
                        }
                        continue;
                    }
                    3 if size == 1 && !reading_graceful.swap(true, Ordering::AcqRel) => {
                        Command::Close
                    }
                    1 if size > 1
                        && !reading_graceful.load(Ordering::Acquire)
                        && !reader_signal.lost() =>
                    {
                        Command::Data(zeroize::Zeroizing::new(frame[1..].to_vec()))
                    }
                    _ => {
                        reader_signal.disconnect();
                        break;
                    }
                };
                let terminal = matches!(command, Command::QuarantineClose);
                if send.try_send(command).is_err() {
                    reader_signal.disconnect();
                    break;
                }
                if terminal { return ControllerReaderExit::QuarantineClose; }
            }
            ControllerReaderExit::Lost
        })?);
    let mut owner = Owner::claim(&root, &host, &package)?;
    let job = Job::new()?;
    let initial = owner.record();
    let token = initial["incarnation"].as_str().unwrap();
    let event = morrow_native_pipe_win::job::RevocationEvent::create(token)?;
    *revocation.lock().map_err(|_| "revocation binding poison")? = Some(event);
    let mut host_args = vec![
        "--supervised-owner".into(),
        token.into(),
        if managed {
            "--managed".into()
        } else {
            database.clone()
        },
        ui.to_string(),
        std::process::id().to_string(),
    ];
    if managed {
        host_args.push(database);
    }
    host_args.push(package.to_string_lossy().into_owned());
    #[cfg(feature="c28-product-session-qualification")]
    if let Some(admission) = &qualification {
        host_args.extend(admission.trailer.iter().cloned());
    }
    let mut child = job.spawn_suspended_prepared_host(&host, &host_args, &root,
        prepared_environment)?;
    let mut record = owner.record();
    record["child_pid"] = child.pid().into();
    record["child_creation_filetime"] = child.creation_filetime()?.into();
    record["phase"] = "Active".into();
    owner.update(record.clone())?;
    {
        let _start = startup.lock().map_err(|_| "startup fence poison")?;
        if signal.lost() || gate.load(Ordering::Acquire) {
            job.terminate()?;
            return Err("UI lost before host resume; owner retained".into());
        }
        child.resume()?;
    }
    let mut stdin = child.stdin.take();
    let proof = stdin.as_ref().ok_or("host stdin missing")?.proof();
    let mut hostout = child.stdout.take().ok_or("host stdout missing")?;
    let mut hosterr = child.stderr.take().ok_or("host stderr missing")?;
    let (events, mut output_events) = mpsc::channel::<(u8, zeroize::Zeroizing<Vec<u8>>)>(8);
    let outdone = Arc::new(AtomicBool::new(false));
    let errdone = Arc::new(AtomicBool::new(false));
    let done = outdone.clone();
    let sender = events.clone();
    let outtask = tokio::spawn(async move {
        let mut buf = [0; 16384];
        loop {
            match hostout.read(&mut buf).await {
                Ok(0) => {
                    done.store(true, Ordering::Release);
                    return Ok::<(), std::io::Error>(());
                }
                Ok(n) => {
                    if sender
                        .send((1, zeroize::Zeroizing::new(buf[..n].to_vec())))
                        .await
                        .is_err()
                    {
                        return Err(std::io::Error::other("output receiver closed"));
                    }
                }
                Err(e) => return Err(e),
            }
        }
    });
    let done = errdone.clone();
    let errtask = tokio::spawn(async move {
        let mut buf = [0; 4096];
        loop {
            match hosterr.read(&mut buf).await {
                Ok(0) => {
                    done.store(true, Ordering::Release);
                    return Ok::<(), std::io::Error>(());
                }
                Ok(n) => {
                    if events
                        .send((3, zeroize::Zeroizing::new(buf[..n].to_vec())))
                        .await
                        .is_err()
                    {
                        return Err(std::io::Error::other("diagnostic receiver closed"));
                    }
                }
                Err(e) => return Err(e),
            }
        }
    });
    let mut output = tokio::io::stdout();
    record["phase"] = "Running".into();
    record["owner_phase"] = "Active".into();
    // Optional capability only. Existing v1/v2 consumers retain their old path.
    record["abnormal_resource_closure_protocol"] = 1.into();
    emit(&mut output,&signal, 2, record.to_string().as_bytes()).await?;
    let mut closing = false;
    let mut fault = false;
    let mut killed = false;
    let mut exit = None;
    let mut drain_at = None;
    let mut persisted_closing = false;
    let mut quarantine_requested = false;
    loop {
        if signal.lost() && !fault {
            fault = true;
            closing = true;
            gate.store(true, Ordering::Release);
            record = owner.record();
            record["phase"] = "Closing".into();
            record["business_gate_revoked"] = true.into();
            record["gate_closed"] = true.into();
            let update = owner.update(record.clone());
            persisted_closing = update.is_ok();
            if let Some(writer) = stdin.take() {
                writer.reclaim_bounded(Duration::from_millis(750));
            }
            record["owner_phase"] = "Retained".into();
            record["phase"] = "ClosingUnconfirmed".into();
            abnormal_closing.store(true, Ordering::Release);
            record["abnormal_resource_closure_protocol"] = 1.into();
            emit(&mut output,&signal, 2, record.to_string().as_bytes()).await?;
        }
        if fault
            && !killed
            && signal
                .lost_elapsed()
                .is_some_and(|v| v >= Duration::from_millis(1000))
        {
            job.terminate()?;
            killed = true;
        }
        if exit.is_none() {
            if let Ok(status) = tokio::time::timeout(Duration::from_millis(1), child.wait()).await {
                exit = Some(status?);
                drain_at = Some(Instant::now());
                if !closing {
                    fault = true;
                    closing = true;
                    signal.disconnect();
                    gate.store(true, Ordering::Release);
                    job.terminate()?;
                    killed = true;
                    record = owner.record();
                    record["phase"] = "Closing".into();
                    record["business_gate_revoked"] = true.into();
                    record["gate_closed"] = true.into();
                    persisted_closing = owner.update(record.clone()).is_ok();
                    abnormal_closing.store(true, Ordering::Release);
                    let mut pending = record.clone();
                    pending["phase"] = "ClosingUnconfirmed".into();
                    pending["owner_phase"] = "Retained".into();
                    pending["abnormal_resource_closure_protocol"] = 1.into();
                    emit(&mut output, &signal, 2, pending.to_string().as_bytes()).await?;
                }
                if let Some(writer) = stdin.take() {
                    writer.reclaim_bounded(Duration::from_millis(750));
                }
            }
        }
        if exit.is_some()
            && outdone.load(Ordering::Acquire)
            && errdone.load(Ordering::Acquire)
            && output_events.is_empty()
            && proof.finished()
            && outtask.is_finished()
            && errtask.is_finished()
            && job.active_processes()? == 0
            && (!abnormal_closing.load(Ordering::Acquire)
                || (quarantine_requested
                    && controller_reader.as_ref().is_some_and(|reader| reader.is_finished())))
        {
            break;
        }
        if drain_at.is_some_and(|v| v.elapsed() >= Duration::from_secs(3)) {
            fault = true;
            job.terminate()?;
            break;
        }
        if signal
            .lost_elapsed()
            .is_some_and(|v| v >= Duration::from_millis(4500))
        {
            fault = true;
            break;
        }
        tokio::select! {
         event=output_events.recv(),if !output_events.is_closed()||!output_events.is_empty()=>{if let Some((kind,bytes))=event{emit(&mut output,&signal,kind,&bytes).await?;}},
         command=commands.recv(),if !closing || (abnormal_closing.load(Ordering::Acquire) && !quarantine_requested)=>{match command{
          Some(Command::QuarantineClose) if closing && fault && abnormal_closing.load(Ordering::Acquire)=>{quarantine_requested=true;gate.store(true,Ordering::Release);},
          Some(Command::Data(bytes)) if !closing=>{if signal.lost()||gate.load(Ordering::Acquire){continue;}let writer=stdin.as_mut().ok_or("closed host input")?;let mut frame=zeroize::Zeroizing::new(Vec::with_capacity(bytes.len()+4));frame.extend_from_slice(&(bytes.len() as u32).to_le_bytes());frame.extend_from_slice(&bytes);tokio::select!{write=writer.write_all(&frame)=>{if write.is_err(){signal.disconnect();gate.store(true,Ordering::Release);}},_=async{while !signal.lost(){tokio::time::sleep(Duration::from_millis(5)).await;}}=>{}}},
          Some(Command::Close) if !closing=>{closing=true;gate.store(true,Ordering::Release);record=owner.record();record["phase"]="Closing".into();record["gate_closed"]=true.into();persisted_closing=owner.update(record.clone()).is_ok();if !persisted_closing{signal.disconnect();continue;}if let Some(writer)=stdin.take(){writer.reclaim_bounded(Duration::from_millis(750));}record["owner_phase"]="Closing".into();emit(&mut output,&signal,2,record.to_string().as_bytes()).await?;},
          Some(_) | None=>signal.disconnect()
         }},
         _=tokio::time::sleep(Duration::from_millis(5))=>{}
        }
    }
    let outjoined = outtask.is_finished() && matches!(outtask.await, Ok(Ok(())));
    let errjoined = errtask.is_finished() && matches!(errtask.await, Ok(Ok(())));
    // Never block the runtime on a live original stdin reader. A panic, live
    // reader or nonterminal result is unconfirmed; the hard deadline remains.
    let controller_joined = if quarantine_requested
        && controller_reader.as_ref().is_some_and(|reader| reader.is_finished())
    {
        matches!(controller_reader.take().unwrap().join(), Ok(ControllerReaderExit::QuarantineClose))
    } else { false };
    let reclaimed = exit.is_some()
        && outdone.load(Ordering::Acquire)
        && errdone.load(Ordering::Acquire)
        && outjoined
        && errjoined
        && proof.closed_and_reaped()
        && job.active_processes()? == 0;
    let normal = !fault
        && closing
        && graceful.load(Ordering::Acquire)
        && exit.as_ref().is_some_and(|v| v.success())
        && persisted_closing
        && reclaimed
        // Linearize complete original normal proof against first controller loss.
        // SQLite commit never owns the loss clock or delays the hard deadline.
        && signal.admit_normal_completion();
    record = owner.record();
    record["resource_reclaimed"] = reclaimed.into();
    record["tree_empty"] = (job.active_processes()? == 0).into();
    record["child_exit"] = exit.is_some().into();
    record["stdout_eof"] = outdone.load(Ordering::Acquire).into();
    record["stderr_eof"] = errdone.load(Ordering::Acquire).into();
    record["control_reaped"] = proof.closed_and_reaped().into();
    record["gate_closed"] = true.into();
    record["child_exit_code"] = exit.as_ref().and_then(|v| v.code()).into();
    record["business_outcome"] = "Unknown".into();
    record["normal_shutdown"] = normal.into();
    record["phase"] = if normal { "Released" } else { "Closing" }.into();
    let mut abnormal_prepared = None;
    let persisted = if !normal && fault && reclaimed && controller_joined
        && quarantine_requested && persisted_closing
    {
        record["business_gate_revoked"] = true.into();
        record["abnormal_resource_closure_proof"] = serde_json::json!({
            "version": 1, "stdout_reader_joined": outjoined,
            "stderr_reader_joined": errjoined, "controller_reader_joined": controller_joined,
        });
        match owner.prepare_abnormal_resource_closure(record.clone()) {
            Ok(prepared) => { abnormal_prepared = Some(prepared.into_extension()); true }
            Err(_) => false, // A possibly committed row is still Unknown; no retry.
        }
    } else {
        owner.update(record.clone()).is_ok()
    };
    record["owner_phase"] = if normal && persisted {
        "Released"
    } else {
        "Retained"
    }
    .into();
    record["phase"] = if normal && persisted {
        "Released"
    } else {
        "ClosingUnconfirmed"
    }
    .into();
    record["durable_proof"] = persisted.into();
    record["normal_shutdown"] = normal.into();
    if let Some(prepared) = abnormal_prepared {
        record["abnormal_resource_closure_prepared"] = prepared;
    }
    emit(&mut output,&signal, 2, record.to_string().as_bytes()).await?;
    Ok(if normal && persisted && !signal.lost() { 0 } else { 2 })
}

/// Reuses this binary as a transient management command; no second supervisor or Job.
async fn management() -> Result<Option<u32>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(mode) = args.first().map(String::as_str) else { return Ok(None); };
    if mode != "--owner-preview" && mode != "--owner-recover" { return Ok(None); }
    if (mode=="--owner-preview" && args.len()!=4) || (mode=="--owner-recover" && (args.len()!=6 || args[5]!="--acknowledge-unknown")) {
        return Err("owner management arguments: mode root host package [preview-token --acknowledge-unknown]".into());
    }
    let root = std::path::Path::new(&args[1]); let host = std::path::Path::new(&args[2]); let package = std::path::Path::new(&args[3]);
    let result = if mode=="--owner-preview" {
        crate::workbench_supervision::recovery::preview(root,host,package)
    } else {
        crate::workbench_supervision::recovery::recover(root,host,package,&args[4],true)
    };
    let (code, value) = match result {
        Ok(value)=>(0,value),
        Err(error)=>(2,serde_json::json!({"version":1,"eligible":false,"recovered":false,"reason":"unconfirmed_or_invalid","detail":error.to_string(),"business_outcome":"Unknown"})),
    };
    let bytes = format!("{}\n", value);
    let mut output = tokio::io::stdout();
    tokio::time::timeout(Duration::from_secs(2), async { output.write_all(bytes.as_bytes()).await?; output.flush().await }).await??;
    Ok(Some(code))
}

#[cfg(test)]
mod quarantine_control_tests {
    use super::*;
    #[test]
    fn quarantine_terminal_is_exact_and_requires_abnormal_closing() {
        assert!(quarantine_control_allowed(&[4], true));
        for frame in [&[][..], &[3][..], &[4, 0][..], &[1, 4][..]] {
            assert!(!quarantine_control_allowed(frame, true));
        }
        assert!(!quarantine_control_allowed(&[4], false));
    }
    #[test]
    fn normal_close_and_heartbeat_are_not_quarantine_terminals() {
        assert!(!quarantine_control_allowed(&[2], true));
        assert!(!quarantine_control_allowed(&[3], true));
        assert!(ControllerReaderExit::Lost != ControllerReaderExit::QuarantineClose);
    }
}
